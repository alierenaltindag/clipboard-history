use crate::injector::traits::PasteInjector;
use async_trait::async_trait;
use tracing::{debug, error};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as XProtoExt;
use x11rb::protocol::xtest::ConnectionExt as XTestExt;
use x11rb::rust_connection::RustConnection;

pub struct X11XTestInjector;

impl X11XTestInjector {
    pub fn new() -> Self {
        Self
    }

    fn find_keycodes(conn: &RustConnection) -> Option<(u8, u8)> {
        let setup = conn.setup();
        let min_keycode = setup.min_keycode;
        let max_keycode = setup.max_keycode;
        let count = max_keycode - min_keycode + 1;

        let mapping = conn
            .get_keyboard_mapping(min_keycode, count)
            .ok()?
            .reply()
            .ok()?;
        let keysyms_per_keycode = mapping.keysyms_per_keycode as usize;

        let mut ctrl_keycode = None;
        let mut v_keycode = None;

        for (idx, chunk) in mapping.keysyms.chunks(keysyms_per_keycode).enumerate() {
            let keycode = min_keycode + idx as u8;
            for &sym in chunk {
                if sym == 0xffe3 || sym == 0xffe4 {
                    // XK_Control_L or XK_Control_R
                    ctrl_keycode = Some(keycode);
                } else if sym == 0x0076 || sym == 0x0056 {
                    // XK_v or XK_V
                    v_keycode = Some(keycode);
                }
            }
        }

        match (ctrl_keycode, v_keycode) {
            (Some(c), Some(v)) => Some((c, v)),
            _ => Some((37, 55)), // Standard PC defaults for Control_L and 'v'
        }
    }
}

#[async_trait]
impl PasteInjector for X11XTestInjector {
    fn name(&self) -> &'static str {
        "X11-XTest"
    }

    fn is_available(&self) -> bool {
        if std::env::var("DISPLAY").is_err() {
            return false;
        }
        if let Ok((conn, _)) = RustConnection::connect(None) {
            conn.xtest_get_version(2, 2).is_ok()
        } else {
            false
        }
    }

    async fn inject(&self) -> bool {
        debug!("Injecting paste via X11 XTest fake input");
        let (conn, _) = match RustConnection::connect(None) {
            Ok(c) => c,
            Err(e) => {
                error!("X11 connection failed: {}", e);
                return false;
            }
        };

        let (ctrl_key, v_key) = Self::find_keycodes(&conn).unwrap_or((37, 55));

        const KEY_PRESS: u8 = 2;
        const KEY_RELEASE: u8 = 3;

        let res = (|| -> Result<(), Box<dyn std::error::Error>> {
            // Press Ctrl
            conn.xtest_fake_input(KEY_PRESS, ctrl_key, 0, 0, 0, 0, 0)?;
            // Press V
            conn.xtest_fake_input(KEY_PRESS, v_key, 0, 0, 0, 0, 0)?;
            // Release V
            conn.xtest_fake_input(KEY_RELEASE, v_key, 0, 0, 0, 0, 0)?;
            // Release Ctrl
            conn.xtest_fake_input(KEY_RELEASE, ctrl_key, 0, 0, 0, 0, 0)?;
            conn.flush()?;
            Ok(())
        })();

        res.is_ok()
    }
}
