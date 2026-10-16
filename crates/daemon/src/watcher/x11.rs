use crate::watcher::traits::{ClipboardWatcher, RawClipboardEvent, SelectionTarget};
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

    async fn read_target_data_with_incr(
        conn: &RustConnection,
        window: u32,
        selection_atom: u32,
        target_atom: u32,
        prop_atom: u32,
        incr_atom: u32,
        timestamp: u32,
    ) -> Option<Vec<u8>> {
        // Request selection conversion into our property with exact timestamp
        if conn
            .convert_selection(window, selection_atom, target_atom, prop_atom, timestamp)
            .is_err()
        {
            return None;
        }
        let _ = conn.flush();

        // Wait briefly for SelectionNotify event
        let start = std::time::Instant::now();
        let mut is_incr = false;

        while start.elapsed() < Duration::from_millis(600) {
            match conn.poll_for_event() {
                Ok(Some(Event::SelectionNotify(notify))) => {
                    let none_atom: u32 = AtomEnum::NONE.into();
                    if notify.property == none_atom {
                        return None;
                    }
                    if let Ok(reply) = conn.get_property(
                        true, // delete property after read
                        window,
                        notify.property,
                        AtomEnum::ANY,
                        0,
                        16 * 1024 * 1024 / 4,
                    ) {
                        if let Ok(prop) = reply.reply() {
                            if prop.type_ == incr_atom {
                                is_incr = true;
                                let _ = conn.flush();
                                break;
                            } else {
                                return Some(prop.value);
                            }
                        }
                    }
                    return None;
                }
                Ok(Some(_)) => {}
                Ok(None) => sleep(Duration::from_millis(5)).await,
                Err(_) => return None,
            }
        }

        if !is_incr {
            return None;
        }

        // Handle ICCCM INCR transfer loop
        let mut total_bytes = Vec::new();
        let incr_start = std::time::Instant::now();

        while incr_start.elapsed() < Duration::from_millis(3000) {
            match conn.poll_for_event() {
                Ok(Some(Event::PropertyNotify(pn))) => {
                    if pn.window == window
                        && pn.atom == prop_atom
                        && pn.state == x11rb::protocol::xproto::Property::NEW_VALUE
                    {
                        if let Ok(reply) = conn.get_property(
                            true,
                            window,
                            prop_atom,
                            AtomEnum::ANY,
                            0,
                            16 * 1024 * 1024 / 4,
                        ) {
                            if let Ok(prop) = reply.reply() {
                                let _ = conn.flush();
                                if prop.value.is_empty() {
                                    // 0-length chunk indicates EOF in INCR protocol
                                    return Some(total_bytes);
                                }
                                total_bytes.extend_from_slice(&prop.value);
                            }
                        }
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => sleep(Duration::from_millis(5)).await,
                Err(_) => return None,
            }
        }

        if !total_bytes.is_empty() {
            Some(total_bytes)
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn fetch_x11_content(
        reader_conn: &RustConnection,
        reader_win: u32,
        selection_atom: u32,
        targets_atom: u32,
        utf8_atom: u32,
        html_atom: u32,
        png_atom: u32,
        jpeg_atom: u32,
        webp_atom: u32,
        uri_list_atom: u32,
        prop_atom: u32,
        incr_atom: u32,
        timestamp: u32,
        selection: SelectionTarget,
    ) -> Option<RawClipboardEvent> {
        // Read available targets
        let mut mime_types = Vec::new();
        if let Some(target_bytes) = Self::read_target_data_with_incr(
            reader_conn,
            reader_win,
            selection_atom,
            targets_atom,
            prop_atom,
            incr_atom,
            timestamp,
        )
        .await
        {
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

        debug!(
            "Discovered X11 ({:?}) MIME types: {:?}",
            selection, mime_types
        );

        let mut text = None;
        let mut html = None;
        let mut image_data = None;

        let img_target = if mime_types.iter().any(|m| m == "image/png") {
            Some(png_atom)
        } else if mime_types.iter().any(|m| m == "image/webp") {
            Some(webp_atom)
        } else if mime_types.iter().any(|m| m == "image/jpeg") {
            Some(jpeg_atom)
        } else {
            None
        };

        if let Some(target) = img_target {
            if let Some(bytes) = Self::read_target_data_with_incr(
                reader_conn,
                reader_win,
                selection_atom,
                target,
                prop_atom,
                incr_atom,
                timestamp,
            )
            .await
            {
                image_data = Some(bytes);
            }
        }

        if mime_types.iter().any(|m| m == "text/html") {
            if let Some(bytes) = Self::read_target_data_with_incr(
                reader_conn,
                reader_win,
                selection_atom,
                html_atom,
                prop_atom,
                incr_atom,
                timestamp,
            )
            .await
            {
                html = String::from_utf8(bytes).ok();
            }
        }

        if mime_types.iter().any(|m| m == "text/uri-list") {
            if let Some(bytes) = Self::read_target_data_with_incr(
                reader_conn,
                reader_win,
                selection_atom,
                uri_list_atom,
                prop_atom,
                incr_atom,
                timestamp,
            )
            .await
            {
                text = String::from_utf8(bytes).ok();
            }
        }

        if text.is_none() {
            if let Some(bytes) = Self::read_target_data_with_incr(
                reader_conn,
                reader_win,
                selection_atom,
                utf8_atom,
                prop_atom,
                incr_atom,
                timestamp,
            )
            .await
            {
                text = String::from_utf8(bytes).ok();
            }
        }

        if text.is_some() || image_data.is_some() || !mime_types.is_empty() {
            Some(RawClipboardEvent {
                selection,
                mime_types,
                text,
                html,
                image_data,
                source_app: None,
            })
        } else {
            None
        }
    }
}

fn x11_err<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::Io(std::io::Error::other(e.to_string()))
}

#[async_trait]
impl ClipboardWatcher for X11Watcher {
    async fn run(&mut self, sender: Sender<RawClipboardEvent>) -> Result<()> {
        info!("Starting native X11 dual clipboard & primary watcher");

        loop {
            let (conn, screen_num) = match RustConnection::connect(None) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to connect to X11 server: {}. Retrying in 2s...", e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let screen = match conn
                .setup()
                .roots
                .get(screen_num)
                .or_else(|| conn.setup().roots.first())
            {
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

            let clipboard_atom = match conn
                .intern_atom(false, b"CLIPBOARD")
                .map_err(x11_err)
                .and_then(|r| r.reply().map_err(x11_err))
            {
                Ok(reply) => reply.atom,
                Err(e) => {
                    warn!("Failed to intern CLIPBOARD atom: {}. Retrying in 2s...", e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let primary_atom: u32 = AtomEnum::PRIMARY.into();

            let targets_atom = conn
                .intern_atom(false, b"TARGETS")
                .map_err(x11_err)?
                .reply()
                .map_err(x11_err)?
                .atom;

            let incr_atom = conn
                .intern_atom(false, b"INCR")
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

            let jpeg_atom = conn
                .intern_atom(false, b"image/jpeg")
                .map_err(x11_err)?
                .reply()
                .map_err(x11_err)?
                .atom;

            let webp_atom = conn
                .intern_atom(false, b"image/webp")
                .map_err(x11_err)?
                .reply()
                .map_err(x11_err)?
                .atom;

            let uri_list_atom = conn
                .intern_atom(false, b"text/uri-list")
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

            // Register for XFixes clipboard & primary change notifications
            let mask = SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE;

            conn.xfixes_select_selection_input(root, clipboard_atom, mask)
                .map_err(x11_err)?;
            conn.xfixes_select_selection_input(root, primary_atom, mask)
                .map_err(x11_err)?;
            conn.flush().map_err(x11_err)?;

            debug!("XFixes CLIPBOARD & PRIMARY listeners successfully registered");

            // Event channel: passes (SelectionTarget, timestamp)
            let (x_tx, mut x_rx) = tokio::sync::mpsc::channel::<(SelectionTarget, u32)>(32);

            let listener_handle = tokio::task::spawn_blocking(move || loop {
                match conn.wait_for_event() {
                    Ok(Event::XfixesSelectionNotify(notify)) => {
                        let none_atom: u32 = AtomEnum::NONE.into();
                        if notify.owner == 0 || notify.owner == none_atom {
                            continue;
                        }

                        let target = if notify.selection == clipboard_atom {
                            SelectionTarget::Clipboard
                        } else if notify.selection == primary_atom {
                            SelectionTarget::Primary
                        } else {
                            continue;
                        };

                        if x_tx
                            .blocking_send((target, notify.selection_timestamp))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!("X11 connection error: {}", e);
                        break;
                    }
                }
            });

            // Async event consumer with trailing debounce for primary selection
            let mut primary_timer: Option<tokio::time::Instant> = None;
            let mut primary_timestamp: u32 = 0;

            loop {
                tokio::select! {
                    msg = x_rx.recv() => {
                        match msg {
                            Some((SelectionTarget::Primary, ts)) => {
                                primary_timestamp = ts;
                                primary_timer = Some(tokio::time::Instant::now() + Duration::from_millis(300));
                            }
                            Some((SelectionTarget::Clipboard, ts)) => {
                                sleep(Duration::from_millis(50)).await;
                                if let Ok((reader_conn, _)) = RustConnection::connect(None) {
                                    if let Ok(reader_win) = reader_conn.generate_id() {
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
                                            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                                        );

                                        if let Some(event) = Self::fetch_x11_content(
                                            &reader_conn,
                                            reader_win,
                                            clipboard_atom,
                                            targets_atom,
                                            utf8_atom,
                                            html_atom,
                                            png_atom,
                                            jpeg_atom,
                                            webp_atom,
                                            uri_list_atom,
                                            prop_atom,
                                            incr_atom,
                                            ts,
                                            SelectionTarget::Clipboard,
                                        ).await {
                                            let _ = sender.send(event).await;
                                        }
                                        let _ = reader_conn.destroy_window(reader_win);
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    _ = async {
                        match primary_timer {
                            Some(deadline) => tokio::time::sleep_until(deadline).await,
                            None => std::future::pending().await,
                        }
                    }, if primary_timer.is_some() => {
                        primary_timer = None;
                        if let Ok((reader_conn, _)) = RustConnection::connect(None) {
                            if let Ok(reader_win) = reader_conn.generate_id() {
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
                                    &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                                );

                                if let Some(event) = Self::fetch_x11_content(
                                    &reader_conn,
                                    reader_win,
                                    primary_atom,
                                    targets_atom,
                                    utf8_atom,
                                    html_atom,
                                    png_atom,
                                    jpeg_atom,
                                    webp_atom,
                                    uri_list_atom,
                                    prop_atom,
                                    incr_atom,
                                    primary_timestamp,
                                    SelectionTarget::Primary,
                                ).await {
                                    if let Some(ref t) = event.text {
                                        if t.trim().len() >= 2 {
                                            let _ = sender.send(event).await;
                                        }
                                    } else if event.image_data.is_some() {
                                        let _ = sender.send(event).await;
                                    }
                                }
                                let _ = reader_conn.destroy_window(reader_win);
                            }
                        }
                    }
                }
            }

            listener_handle.abort();
            warn!("X11 event loop disconnected. Re-establishing connection in 1s...");
            sleep(Duration::from_secs(1)).await;
        }
    }
}
