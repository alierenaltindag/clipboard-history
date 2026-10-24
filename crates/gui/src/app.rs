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
    app: libadwaita::Application,
}

#[cfg(feature = "gtk")]
impl ClipboardApp {
    pub fn new() -> Self {
        let app = libadwaita::Application::builder()
            .application_id("com.antigravity.ClipboardHistory")
            .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
            .build();

        let window_cell: Rc<RefCell<Option<ClipboardWindow>>> = Rc::new(RefCell::new(None));

        let win_startup = Rc::clone(&window_cell);
        app.connect_startup(move |app| {
            info!("GUI Application startup");
            // Hold application in memory indefinitely so hiding the window does not terminate the app
            Box::leak(Box::new(app.hold()));
            let config = AppConfig::load_or_default();
            let win = ClipboardWindow::new(app, config);
            *win_startup.borrow_mut() = Some(win);

            // Periodically check for incoming pairing requests and display actionable prompt modal
            let active_dialogs: Rc<RefCell<std::collections::HashSet<String>>> =
                Rc::new(RefCell::new(std::collections::HashSet::new()));
            let active_dialog_windows: Rc<
                RefCell<std::collections::HashMap<String, gtk4::Window>>,
            > = Rc::new(RefCell::new(std::collections::HashMap::new()));
            let win_ref_dialog = Rc::clone(&win_startup);

            glib::timeout_add_local(std::time::Duration::from_millis(1500), move || {
                let active_set = Rc::clone(&active_dialogs);
                let active_wins = Rc::clone(&active_dialog_windows);
                let win_r = Rc::clone(&win_ref_dialog);
                glib::MainContext::default().spawn_local(async move {
                    use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
                    if let Ok(client) = IpcClient::connect().await {
                        if let Ok(IpcResponse::PendingPairings(requests)) =
                            client.send(&IpcRequest::GetPendingPairingRequests).await
                        {
                            let current_ids: std::collections::HashSet<String> =
                                requests.iter().map(|r| r.request_id.clone()).collect();

                            // Close any open pairing dialogs whose requests are no longer pending
                            active_wins.borrow_mut().retain(|id, win| {
                                if !current_ids.contains(id) {
                                    win.close();
                                    false
                                } else {
                                    true
                                }
                            });
                            active_set
                                .borrow_mut()
                                .retain(|id| current_ids.contains(id));

                            for req in requests {
                                if !active_set.borrow().contains(&req.request_id) {
                                    active_set.borrow_mut().insert(req.request_id.clone());
                                    let req_id = req.request_id.clone();
                                    let active_rm = Rc::clone(&active_set);
                                    let win_guard = win_r.borrow();
                                    let parent = win_guard.as_ref().map(|w| &w.window);
                                    let dlg = crate::pairing_dialog::PairingDialog::show(
                                        parent, req, active_rm,
                                    );
                                    drop(win_guard);
                                    active_wins.borrow_mut().insert(req_id, dlg);
                                }
                            }
                        }
                    }
                });
                glib::ControlFlow::Continue
            });
        });

        let win_activate = Rc::clone(&window_cell);
        app.connect_activate(move |_| {
            if let Some(win) = win_activate.borrow().as_ref() {
                if win.window.is_visible() && win.window.is_active() {
                    win.window.set_visible(false);
                } else {
                    win.present_near_cursor();
                }
            }
        });

        let win_cmd = Rc::clone(&window_cell);
        app.connect_command_line(move |_, cmd_line| {
            let args = cmd_line.arguments();
            let has_toggle = args.iter().any(|a| a == "--toggle" || a == "-t");
            let has_settings = args.iter().any(|a| a == "--settings" || a == "-s");

            if has_settings {
                let cfg = AppConfig::load_or_default();
                crate::settings_dialog::SettingsDialog::show(
                    win_cmd.borrow().as_ref().map(|w| &w.window),
                    cfg,
                );
            } else if has_toggle {
                if let Some(win) = win_cmd.borrow().as_ref() {
                    if win.window.is_visible() && win.window.is_active() {
                        win.window.set_visible(false);
                    } else {
                        win.present_near_cursor();
                    }
                }
            } else if let Some(win) = win_cmd.borrow().as_ref() {
                win.present_near_cursor();
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
