#![cfg(feature = "gtk")]

use clipboard_history_core::config::AppConfig;
use clipboard_history_core::ipc::{IpcClient, IpcRequest};
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use tracing::info;

pub struct SettingsDialog;

impl SettingsDialog {
    pub fn show(parent: &gtk4::Window, config: AppConfig) {
        let dialog = libadwaita::PreferencesWindow::new();
        dialog.set_transient_for(Some(parent));
        dialog.set_modal(true);
        dialog.set_title(Some("Clipboard Preferences"));
        dialog.set_default_width(620);
        dialog.set_default_height(540);

        // ==========================================
        // PAGE 1: General
        // ==========================================
        let general_page = libadwaita::PreferencesPage::new();
        general_page.set_title("General");
        general_page.set_icon_name(Some("preferences-system-symbolic"));

        // Retention & Capacity Group
        let general_group = libadwaita::PreferencesGroup::new();
        general_group.set_title("Retention & Capacity");
        general_group.set_description(Some(
            "Manage how many entries to preserve in local encrypted storage",
        ));

        let max_entries_row = libadwaita::SpinRow::new(
            Some(&gtk4::Adjustment::new(
                config.general.max_entries as f64,
                50.0,
                5000.0,
                50.0,
                100.0,
                0.0,
            )),
            1.0,
            0,
        );
        max_entries_row.set_title("Max History Entries");
        max_entries_row
            .set_subtitle("Older unpinned entries are automatically evicted when limit is reached");
        general_group.add(&max_entries_row);

        let retention_row = libadwaita::SpinRow::new(
            Some(&gtk4::Adjustment::new(
                config.general.retention_days as f64,
                1.0,
                365.0,
                1.0,
                7.0,
                0.0,
            )),
            1.0,
            0,
        );
        retention_row.set_title("Retention Days");
        retention_row.set_subtitle("Automatically delete entries older than this duration");
        general_group.add(&retention_row);

        general_page.add(&general_group);

        // Pasting Group
        let paste_group = libadwaita::PreferencesGroup::new();
        paste_group.set_title("Pasting Behavior");
        paste_group.set_description(Some(
            "Configure how selected entries are injected into active apps",
        ));

        let auto_paste_row = libadwaita::SwitchRow::new();
        auto_paste_row.set_title("Direct Keystroke Auto-Paste");
        auto_paste_row
            .set_subtitle("Automatically synthesize Ctrl+V keystrokes when an item is chosen");
        auto_paste_row.set_active(config.paste.auto_paste);
        paste_group.add(&auto_paste_row);

        general_page.add(&paste_group);
        dialog.add(&general_page);

        // ==========================================
        // PAGE 2: Privacy & Security
        // ==========================================
        let security_page = libadwaita::PreferencesPage::new();
        security_page.set_title("Privacy & Security");
        security_page.set_icon_name(Some("security-high-symbolic"));

        // Protection Rules Group
        let security_group = libadwaita::PreferencesGroup::new();
        security_group.set_title("Protection Rules");
        security_group.set_description(Some(
            "Prevent sensitive passwords and private browsing data from leaking",
        ));

        let pwd_row = libadwaita::SwitchRow::new();
        pwd_row.set_title("Protect Password Managers");
        pwd_row.set_subtitle("Ignore clipboard captures from KeePassXC, 1Password, and Bitwarden");
        pwd_row.set_active(config.security.ignore_password_managers);
        security_group.add(&pwd_row);

        let incognito_row = libadwaita::SwitchRow::new();
        incognito_row.set_title("Ignore Private & Incognito Browsing");
        incognito_row
            .set_subtitle("Do not record copies made inside incognito or private browsing windows");
        incognito_row.set_active(config.security.ignore_incognito_windows);
        security_group.add(&incognito_row);

        let clean_urls_row = libadwaita::SwitchRow::new();
        clean_urls_row.set_title("URL De-Tracker & Privacy Cleaner");
        clean_urls_row.set_subtitle(
            "Strip tracking query parameters (utm_*, fbclid, gclid) from copied links",
        );
        clean_urls_row.set_active(config.security.auto_clean_tracking_urls);
        security_group.add(&clean_urls_row);

        security_page.add(&security_group);

        // Application Filter Group
        let filter_group = libadwaita::PreferencesGroup::new();
        filter_group.set_title("Application Filtering");
        filter_group.set_description(Some(
            "Control which desktop applications are monitored for clipboard events",
        ));

        let mode_row = libadwaita::ComboRow::new();
        let model = gtk4::StringList::new(&[
            "Blacklist (Ignore specified apps)",
            "Whitelist (Only save from specified apps)",
        ]);
        mode_row.set_model(Some(&model));
        mode_row.set_title("Filter Mode");
        mode_row.set_selected(match config.security.app_filter_mode {
            clipboard_history_core::config::AppFilterMode::Blacklist => 0,
            clipboard_history_core::config::AppFilterMode::Whitelist => 1,
        });
        filter_group.add(&mode_row);

        let list_row = libadwaita::EntryRow::new();
        list_row.set_title("Filtered Apps (comma-separated)");
        list_row.set_text(&config.security.app_filter_list.join(", "));
        filter_group.add(&list_row);

        security_page.add(&filter_group);
        dialog.add(&security_page);

        // ==========================================
        // PAGE 3: Network Sync
        // ==========================================
        let sync_page = libadwaita::PreferencesPage::new();
        sync_page.set_title("Network Sync");
        sync_page.set_icon_name(Some("network-wireless-symbolic"));

        let sync_group = libadwaita::PreferencesGroup::new();
        sync_group.set_title("P2P Local Network Sync");
        sync_group.set_description(Some(
            "Synchronize clipboard items securely across LAN devices using AES-256-GCM",
        ));

        let sync_toggle_row = libadwaita::SwitchRow::new();
        sync_toggle_row.set_title("Enable LAN Clipboard Sync");
        sync_toggle_row
            .set_subtitle("Automatically discover and sync clipboard with trusted devices on LAN");
        sync_toggle_row.set_active(config.sync.enabled);
        sync_group.add(&sync_toggle_row);

        let pin_row = libadwaita::EntryRow::new();
        pin_row.set_title("Pairing PIN");
        pin_row.set_text(&config.sync.pairing_pin);
        sync_group.add(&pin_row);

        sync_page.add(&sync_group);
        dialog.add(&sync_page);

        // Connect setting listeners to save to config.toml and sync with daemon via IPC
        let cfg_cell = Rc::new(RefCell::new(config));

        let cfg_pwd = Rc::clone(&cfg_cell);
        pwd_row.connect_active_notify(move |row| {
            cfg_pwd.borrow_mut().security.ignore_password_managers = row.is_active();
            Self::save_and_sync(&cfg_pwd.borrow());
        });

        let cfg_incog = Rc::clone(&cfg_cell);
        incognito_row.connect_active_notify(move |row| {
            cfg_incog.borrow_mut().security.ignore_incognito_windows = row.is_active();
            Self::save_and_sync(&cfg_incog.borrow());
        });

        let cfg_clean = Rc::clone(&cfg_cell);
        clean_urls_row.connect_active_notify(move |row| {
            cfg_clean.borrow_mut().security.auto_clean_tracking_urls = row.is_active();
            Self::save_and_sync(&cfg_clean.borrow());
        });

        let cfg_mode = Rc::clone(&cfg_cell);
        mode_row.connect_selected_notify(move |row| {
            let mode = if row.selected() == 1 {
                clipboard_history_core::config::AppFilterMode::Whitelist
            } else {
                clipboard_history_core::config::AppFilterMode::Blacklist
            };
            cfg_mode.borrow_mut().security.app_filter_mode = mode;
            Self::save_and_sync(&cfg_mode.borrow());
        });

        let cfg_list = Rc::clone(&cfg_cell);
        list_row.connect_changed(move |row| {
            let text = row.text().to_string();
            let items = text
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            cfg_list.borrow_mut().security.app_filter_list = items;
            Self::save_and_sync(&cfg_list.borrow());
        });

        let cfg_paste = Rc::clone(&cfg_cell);
        auto_paste_row.connect_active_notify(move |row| {
            cfg_paste.borrow_mut().paste.auto_paste = row.is_active();
            Self::save_and_sync(&cfg_paste.borrow());
        });

        let cfg_entries = Rc::clone(&cfg_cell);
        max_entries_row.connect_value_notify(move |row| {
            cfg_entries.borrow_mut().general.max_entries = row.value() as usize;
            Self::save_and_sync(&cfg_entries.borrow());
        });

        let cfg_ret = Rc::clone(&cfg_cell);
        retention_row.connect_value_notify(move |row| {
            cfg_ret.borrow_mut().general.retention_days = row.value() as u32;
            Self::save_and_sync(&cfg_ret.borrow());
        });

        let cfg_sync = Rc::clone(&cfg_cell);
        sync_toggle_row.connect_active_notify(move |row| {
            cfg_sync.borrow_mut().sync.enabled = row.is_active();
            Self::save_and_sync(&cfg_sync.borrow());
        });

        let debounce_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let deb_clone = Rc::clone(&debounce_timer);
        let cfg_pin = Rc::clone(&cfg_cell);
        pin_row.connect_changed(move |row| {
            if let Some(source) = deb_clone.borrow_mut().take() {
                source.remove();
            }
            let new_pin = row.text().to_string();
            let cfg_ref = Rc::clone(&cfg_pin);
            let deb_ref = Rc::clone(&deb_clone);
            let source_id =
                glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
                    deb_ref.borrow_mut().take();
                    cfg_ref.borrow_mut().sync.pairing_pin = new_pin;
                    Self::save_and_sync(&cfg_ref.borrow());
                });
            *deb_clone.borrow_mut() = Some(source_id);
        });

        dialog.present();
    }

    fn save_and_sync(cfg: &AppConfig) {
        if let Err(e) = cfg.save(AppConfig::config_path()) {
            tracing::warn!("Failed to save config: {}", e);
        } else {
            info!("Updated configuration saved to disk");
        }

        let to_send = cfg.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Ok(client) = IpcClient::connect().await {
                let _ = client
                    .send(&IpcRequest::UpdateConfig {
                        config: Box::new(to_send),
                    })
                    .await;
                info!("Sent updated configuration to daemon via IPC");
            }
        });
    }
}
