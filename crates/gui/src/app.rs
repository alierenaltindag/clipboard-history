#[cfg(feature = "gtk")]
use crate::window::ClipboardWindow;
#[cfg(feature = "gtk")]
use clipboard_history_core::config::AppConfig;
#[cfg(feature = "gtk")]
use gtk4::prelude::*;
#[cfg(feature = "gtk")]
use std::cell::RefCell;
#[cfg(feature = "gtk")]
use std::rc::Rc;
#[cfg(feature = "gtk")]
use tracing::info;

#[cfg(feature = "gtk")]
pub struct ClipboardApp {
    app: gtk4::Application,
}

#[cfg(feature = "gtk")]
impl ClipboardApp {
    pub fn new() -> Self {
        let app = gtk4::Application::builder()
            .application_id("com.antigravity.ClipboardHistory")
            .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
            .build();

        let window_cell: Rc<RefCell<Option<ClipboardWindow>>> = Rc::new(RefCell::new(None));

        let win_startup = Rc::clone(&window_cell);
        app.connect_startup(move |app| {
            info!("GUI Application startup");
            let config = AppConfig::load_or_default();
            let win = ClipboardWindow::new(app, config);
            *win_startup.borrow_mut() = Some(win);
        });

        let win_activate = Rc::clone(&window_cell);
        app.connect_activate(move |_| {
            if let Some(win) = win_activate.borrow().as_ref() {
                if win.window.is_visible() {
                    win.window.set_visible(false);
                } else {
                    win.present_near_cursor();
                }
            }
        });

        let win_cmd = Rc::clone(&window_cell);
        app.connect_command_line(move |_, cmd_line| {
            let args = cmd_line.arguments();
            let has_toggle = args.iter().any(|a| a == "--toggle");
            if has_toggle {
                if let Some(win) = win_cmd.borrow().as_ref() {
                    if win.window.is_visible() {
                        win.window.set_visible(false);
                    } else {
                        win.present_near_cursor();
                    }
                }
            }
            0
        });

        Self { app }
    }

    pub fn run(&self) -> i32 {
        self.app
            .run_with_args(&std::env::args().collect::<Vec<_>>())
            .into()
    }
}
