#![cfg(feature = "gtk")]

use clipboard_history_core::service::{collect_diagnostics, smart_start_daemon, DaemonDiagnostic};
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::rc::Rc;

pub fn show_diagnostics_dialog(parent: Option<&gtk4::Window>) {
    let window = libadwaita::Window::builder()
        .title("Service Diagnostics & Troubleshoot")
        .default_width(580)
        .default_height(520)
        .modal(true)
        .build();

    if let Some(p) = parent {
        window.set_transient_for(Some(p));
    }

    let root_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    // Header bar
    let header_bar = libadwaita::HeaderBar::new();
    header_bar.set_show_end_title_buttons(true);

    let copy_btn = gtk4::Button::from_icon_name("edit-copy-symbolic");
    copy_btn.set_tooltip_text(Some("Copy Diagnostics to Clipboard"));
    header_bar.pack_start(&copy_btn);

    root_box.append(&header_bar);

    // Loading / Content Stack
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);

    let spinner_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    spinner_box.set_halign(gtk4::Align::Center);
    spinner_box.set_valign(gtk4::Align::Center);
    spinner_box.set_vexpand(true);

    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(32, 32);
    let loading_label = gtk4::Label::new(Some("Collecting system & daemon diagnostics..."));
    loading_label.add_css_class("dim-label");

    spinner_box.append(&spinner);
    spinner_box.append(&loading_label);
    stack.add_named(&spinner_box, Some("loading"));

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    content_box.set_margin_start(16);
    content_box.set_margin_end(16);
    content_box.set_margin_top(16);
    content_box.set_margin_bottom(16);
    scrolled.set_child(Some(&content_box));

    stack.add_named(&scrolled, Some("content"));
    root_box.append(&stack);
    window.set_content(Some(&root_box));

    let window_ref = window.clone();
    let diag_report_holder = Rc::new(std::cell::RefCell::new(String::new()));
    let diag_report_copy = Rc::clone(&diag_report_holder);

    copy_btn.connect_clicked(move |btn| {
        let text = diag_report_copy.borrow().clone();
        if !text.is_empty() {
            let clipboard = btn.clipboard();
            clipboard.set_text(&text);
            btn.set_tooltip_text(Some("Copied to clipboard!"));
        }
    });

    window.present();

    // Load diagnostics asynchronously
    glib::MainContext::default().spawn_local(async move {
        let diag = collect_diagnostics().await;
        *diag_report_holder.borrow_mut() = diag.to_report_string();

        populate_diagnostics_ui(&content_box, &diag, &window_ref);
        stack.set_visible_child_name("content");
    });
}

fn populate_diagnostics_ui(
    container: &gtk4::Box,
    diag: &DaemonDiagnostic,
    parent_window: &libadwaita::Window,
) {
    // Group 1: Binary & Socket
    let core_group = libadwaita::PreferencesGroup::new();
    core_group.set_title("Service Binaries & IPC");

    let daemon_row = libadwaita::ActionRow::new();
    daemon_row.set_title("Daemon Executable");
    if let Some(ref path) = diag.daemon_binary {
        daemon_row.set_subtitle(&path.display().to_string());
        let icon = gtk4::Image::from_icon_name("emblem-ok-symbolic");
        icon.add_css_class("success");
        daemon_row.add_suffix(&icon);
    } else {
        daemon_row.set_subtitle("Not Found in PATH or standard paths");
        let icon = gtk4::Image::from_icon_name("dialog-error-symbolic");
        icon.add_css_class("error");
        daemon_row.add_suffix(&icon);
    }
    core_group.add(&daemon_row);

    let wl_row = libadwaita::ActionRow::new();
    wl_row.set_title("Wayland Helper (wl-paste)");
    if diag.wl_paste_available {
        wl_row.set_subtitle("Installed and available in PATH");
        let icon = gtk4::Image::from_icon_name("emblem-ok-symbolic");
        icon.add_css_class("success");
        wl_row.add_suffix(&icon);
    } else {
        wl_row.set_subtitle("Not found — Install wl-clipboard for Wayland clipboard support");
        let icon = gtk4::Image::from_icon_name("dialog-warning-symbolic");
        icon.add_css_class("warning");
        wl_row.add_suffix(&icon);
    }
    core_group.add(&wl_row);

    let socket_row = libadwaita::ActionRow::new();
    socket_row.set_title("Unix IPC Socket");
    let sock_status = match (diag.socket_exists, diag.socket_responsive) {
        (true, true) => "Active & Listening",
        (true, false) => "File exists but unresponsive (stale)",
        (false, _) => "Not created (daemon not started)",
    };
    socket_row.set_subtitle(&format!("{} — {}", diag.socket_path.display(), sock_status));
    let sock_icon = if diag.socket_responsive {
        let img = gtk4::Image::from_icon_name("emblem-ok-symbolic");
        img.add_css_class("success");
        img
    } else {
        let img = gtk4::Image::from_icon_name("dialog-warning-symbolic");
        img.add_css_class("warning");
        img
    };
    socket_row.add_suffix(&sock_icon);
    core_group.add(&socket_row);
    container.append(&core_group);

    // Group 2: Systemd Integration
    let systemd_group = libadwaita::PreferencesGroup::new();
    systemd_group.set_title("Systemd User Service");

    let unit_row = libadwaita::ActionRow::new();
    unit_row.set_title("Unit File Location");
    unit_row.set_subtitle(
        &diag
            .systemd_unit_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "Not found (Will be auto-installed when started)".to_string()),
    );
    systemd_group.add(&unit_row);

    let status_row = libadwaita::ActionRow::new();
    status_row.set_title("Systemd Status");
    status_row.set_subtitle(&format!(
        "Active: {} ({})",
        diag.is_active_in_systemd, diag.systemd_unit_status
    ));
    systemd_group.add(&status_row);
    container.append(&systemd_group);

    // Group 3: Display Environment
    let env_group = libadwaita::PreferencesGroup::new();
    env_group.set_title("Display Environment");

    let disp_row = libadwaita::ActionRow::new();
    disp_row.set_title("Wayland / X11 Session");
    disp_row.set_subtitle(&format!(
        "WAYLAND_DISPLAY={:?}, DISPLAY={:?}, TYPE={:?}",
        diag.wayland_display.as_deref().unwrap_or("<none>"),
        diag.x11_display.as_deref().unwrap_or("<none>"),
        diag.session_type.as_deref().unwrap_or("<none>")
    ));
    env_group.add(&disp_row);
    container.append(&env_group);

    // Group 4: Recent Logs
    if !diag.recent_logs.is_empty() {
        let log_group = libadwaita::PreferencesGroup::new();
        log_group.set_title("Recent Service Logs");
        log_group.set_description(Some("From journalctl --user -u clipboard-history.service"));

        let log_view = gtk4::TextView::new();
        log_view.set_editable(false);
        log_view.set_cursor_visible(false);
        log_view.set_monospace(true);
        log_view.set_wrap_mode(gtk4::WrapMode::Char);
        log_view.add_css_class("card");
        log_view.set_margin_top(4);
        log_view.set_margin_bottom(4);

        let buffer = log_view.buffer();
        let log_text = diag.recent_logs.join("\n");
        buffer.set_text(&log_text);

        let log_scroller = gtk4::ScrolledWindow::builder()
            .min_content_height(140)
            .max_content_height(200)
            .child(&log_view)
            .build();

        log_group.add(&log_scroller);
        container.append(&log_group);
    }

    // Action buttons at bottom
    let action_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    action_box.set_halign(gtk4::Align::End);
    action_box.set_margin_top(8);

    let start_now_btn = gtk4::Button::with_label("Auto-Repair & Start Service");
    start_now_btn.add_css_class("suggested-action");
    start_now_btn.add_css_class("pill");

    let s_btn = start_now_btn.clone();
    let win_close = parent_window.clone();

    start_now_btn.connect_clicked(move |_| {
        s_btn.set_sensitive(false);
        s_btn.set_label("Starting...");
        let win_c = win_close.clone();
        let b = s_btn.clone();

        glib::MainContext::default().spawn_local(async move {
            match smart_start_daemon().await {
                Ok(_) => {
                    win_c.close();
                }
                Err(e) => {
                    b.set_sensitive(true);
                    b.set_label("Start Failed — Try Again");
                    tracing::error!("Diagnostics start failed: {}", e);
                }
            }
        });
    });

    action_box.append(&start_now_btn);
    container.append(&action_box);
}
