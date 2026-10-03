use crate::watcher::traits::{ClipboardWatcher, RawClipboardEvent};
use async_trait::async_trait;
use clipboard_history_core::error::{CoreError, Result};
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{ConnectionExt as XFixesConnectionExt, SelectionEventMask};
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as XProtoConnectionExt, CreateWindowAux, EventMask, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub struct X11Watcher;

impl X11Watcher {
    pub fn new() -> Self {
        Self
    }

    fn read_target_data(
        conn: &RustConnection,
        window: u32,
        clipboard_atom: u32,
        target_atom: u32,
        prop_atom: u32,
    ) -> Option<Vec<u8>> {
        // Request selection conversion into our property
        if conn
            .convert_selection(
                window,
                clipboard_atom,
                target_atom,
                prop_atom,
                x11rb::CURRENT_TIME,
            )
            .is_err()
        {
            return None;
        }
        let _ = conn.flush();

        // Wait briefly for SelectionNotify event
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(500) {
            match conn.poll_for_event() {
                Ok(Some(Event::SelectionNotify(notify))) => {
                    let none_atom: u32 = AtomEnum::NONE.into();
                    if notify.property == none_atom {
                        return None;
                    }
                    // Fetch property
                    if let Ok(reply) = conn.get_property(
                        true, // delete property after read
                        window,
                        notify.property,
                        AtomEnum::ANY,
                        0,
                        16 * 1024 * 1024 / 4, // up to 16 MB
                    ) {
                        if let Ok(prop) = reply.reply() {
                            return Some(prop.value);
                        }
                    }
                    return None;
                }
                Ok(Some(_)) => {}
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(_) => return None,
            }
        }
        None
    }
}

fn x11_err<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Io(std::io::Error::other(e.to_string()))
}

#[async_trait]
impl ClipboardWatcher for X11Watcher {
    async fn run(&mut self, sender: Sender<RawClipboardEvent>) -> Result<()> {
        info!("Starting native X11 XFixes clipboard watcher");

        loop {
            let (conn, screen_num) = match RustConnection::connect(None) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to connect to X11 server: {}. Retrying in 2s...", e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let screen = match conn.setup().roots.get(screen_num).or_else(|| conn.setup().roots.first()) {
                Some(s) => s,
                None => {
                    error!("No valid X11 screens found. Retrying in 2s...");
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let root = screen.root;
            let root_depth = screen.root_depth;
            let root_visual = screen.root_visual;

            let version = match conn.xfixes_query_version(5, 0) {
                Ok(v) => v,
                Err(e) => {
                    warn!("XFixes query failed: {}. Retrying in 2s...", e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };
            let _ = version.reply();

            let clipboard_atom = match conn.intern_atom(false, b"CLIPBOARD").map_err(x11_err).and_then(|r| r.reply().map_err(x11_err)) {
                Ok(reply) => reply.atom,
                Err(e) => {
                    warn!("Failed to intern CLIPBOARD atom: {}. Retrying in 2s...", e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

        let targets_atom = conn
            .intern_atom(false, b"TARGETS")
            .map_err(x11_err)?
            .reply()
            .map_err(x11_err)?
            .atom;

        let utf8_atom = conn
            .intern_atom(false, b"UTF8_STRING")
            .map_err(x11_err)?
            .reply()
            .map_err(x11_err)?
            .atom;

        let html_atom = conn
            .intern_atom(false, b"text/html")
            .map_err(x11_err)?
            .reply()
            .map_err(x11_err)?
            .atom;

        let png_atom = conn
            .intern_atom(false, b"image/png")
            .map_err(x11_err)?
            .reply()
            .map_err(x11_err)?
            .atom;

        let prop_atom = conn
            .intern_atom(false, b"XCLIP_DATA")
            .map_err(x11_err)?
            .reply()
            .map_err(x11_err)?
            .atom;

        // Create an invisible listener window
        let win = conn.generate_id().map_err(x11_err)?;
        conn.create_window(
            root_depth,
            win,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            root_visual,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(x11_err)?;

        // Register for XFixes clipboard change notifications
        conn.xfixes_select_selection_input(
            root,
            clipboard_atom,
            SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE,
        )
        .map_err(x11_err)?;

        conn.flush().map_err(x11_err)?;

        debug!("XFixes clipboard listener successfully registered");

        // Spawn blocking loop in tokio task to prevent blocking async runtime and allow clean tracking/joining
        let (x_tx, mut x_rx) = tokio::sync::mpsc::channel::<()>(16);

        let listener_handle = tokio::task::spawn_blocking(move || {
            loop {
                match conn.wait_for_event() {
                    Ok(Event::XfixesSelectionNotify(notify)) => {
                        if notify.selection == clipboard_atom {
                            debug!("XFixes SelectionNotify received");
                            if x_tx.blocking_send(()).is_err() {
                                debug!("X11 event receiver dropped, terminating worker thread");
                                break;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!("X11 connection error: {}", e);
                        break;
                    }
                }
            }
        });

        // Event processing loop with debounce
        while let Some(()) = x_rx.recv().await {
            // Debounce: sleep 50ms in case rapid events arrive
            sleep(Duration::from_millis(50)).await;
            while x_rx.try_recv().is_ok() {}

            // Reconnect lightweight reader or read selection
            let reader_conn = match RustConnection::connect(None) {
                Ok((c, _)) => c,
                Err(e) => {
                    warn!("Failed to open reader X11 connection: {}", e);
                    continue;
                }
            };

            let reader_win = match reader_conn.generate_id() {
                Ok(id) => id,
                Err(_) => continue,
            };

            let _ = reader_conn.create_window(
                root_depth,
                reader_win,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                root_visual,
                &CreateWindowAux::new(),
            );

            // Read available targets
            let mut mime_types = Vec::new();
            if let Some(target_bytes) = Self::read_target_data(
                &reader_conn,
                reader_win,
                clipboard_atom,
                targets_atom,
                prop_atom,
            ) {
                let atom_count = target_bytes.len() / 4;
                for i in 0..atom_count {
                    let chunk = &target_bytes[i * 4..(i + 1) * 4];
                    let atom = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    if let Ok(name_reply) = reader_conn.get_atom_name(atom) {
                        if let Ok(name) = name_reply.reply() {
                            if let Ok(s) = String::from_utf8(name.name) {
                                mime_types.push(s);
                            }
                        }
                    }
                }
            }

            debug!("Discovered clipboard MIME types: {:?}", mime_types);

            let mut text = None;
            let mut html = None;
            let mut image_data = None;

            if mime_types
                .iter()
                .any(|m| m == "image/png" || m == "image/jpeg")
            {
                if let Some(bytes) = Self::read_target_data(
                    &reader_conn,
                    reader_win,
                    clipboard_atom,
                    png_atom,
                    prop_atom,
                ) {
                    image_data = Some(bytes);
                }
            }

            if mime_types.iter().any(|m| m == "text/html") {
                if let Some(bytes) = Self::read_target_data(
                    &reader_conn,
                    reader_win,
                    clipboard_atom,
                    html_atom,
                    prop_atom,
                ) {
                    html = String::from_utf8(bytes).ok();
                }
            }

            // UTF8 string
            if let Some(bytes) = Self::read_target_data(
                &reader_conn,
                reader_win,
                clipboard_atom,
                utf8_atom,
                prop_atom,
            ) {
                text = String::from_utf8(bytes).ok();
            }

            let _ = reader_conn.destroy_window(reader_win);

            if text.is_some() || image_data.is_some() || !mime_types.is_empty() {
                let event = RawClipboardEvent {
                    mime_types,
                    text,
                    html,
                    image_data,
                    source_app: None,
                };
                let _ = sender.send(event).await;
            }
            }
            listener_handle.abort();
            warn!("X11 event loop disconnected. Re-establishing connection in 1s...");
            sleep(Duration::from_secs(1)).await;
        }
    }
}
