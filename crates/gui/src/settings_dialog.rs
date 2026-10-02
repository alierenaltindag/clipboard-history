#[cfg(feature = "gtk")]
use clipboard_history_core::config::AppConfig;
#[cfg(feature = "gtk")]
use clipboard_history_core::ipc::{IpcClient, IpcRequest};
#[cfg(feature = "gtk")]
use gtk4::prelude::*;
#[cfg(feature = "gtk")]
use libadwaita::prelude::*;
#[cfg(feature = "gtk")]
use std::cell::RefCell;
#[cfg(feature = "gtk")]
use std::rc::Rc;
#[cfg(feature = "gtk")]
use tracing::info;

#[cfg(feature = "gtk")]
pub struct SettingsDialog;

#[cfg(feature = "gtk")]
impl SettingsDialog {
    pub fn show(parent: &gtk4::Window, config: AppConfig) {
        let dialog = libadwaita::PreferencesWindow::new();
        dialog.set_transient_for(Some(parent));
        dialog.set_modal(true);
        dialog.set_title(Some("Clipboard Preferences"));

        let page = libadwaita::PreferencesPage::new();
        page.set_title("General");
        page.set_icon_name(Some("preferences-system-symbolic"));

        // General Group
        let general_group = libadwaita::PreferencesGroup::new();
        general_group.set_title("Retention & Capacity");

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
        max_entries_row.set_subtitle("Older unpinned entries are evicted when limit is reached");
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

        page.add(&general_group);

        // Security & Privacy Group
        let security_group = libadwaita::PreferencesGroup::new();
        security_group.set_title("Privacy & Security");

        // 1. Password manager protection toggle (default: true)
        let pwd_row = libadwaita::SwitchRow::new();
        pwd_row.set_title("Protect Password Managers");
        pwd_row.set_subtitle(
            "Do not save copies from KeePassXC, 1Password, and Bitwarden (Default: On)",
        );
        pwd_row.set_active(config.security.ignore_password_managers);
        security_group.add(&pwd_row);

        // 2. Incognito / Private Browsing protection toggle (default: true)
        let incognito_row = libadwaita::SwitchRow::new();
        incognito_row.set_title("Ignore Incognito & Private Windows");
        incognito_row.set_subtitle(
            "Do not save copies while in private browsing or incognito tabs (Default: On)",
        );
        incognito_row.set_active(config.security.ignore_incognito_windows);
        security_group.add(&incognito_row);

        // 3. Direct keystroke auto-paste toggle
        let auto_paste_row = libadwaita::SwitchRow::new();
        auto_paste_row.set_title("Direct Keystroke Paste");
        auto_paste_row.set_subtitle("Automatically synthesize Ctrl+V when an entry is selected");
        auto_paste_row.set_active(config.paste.auto_paste);
        security_group.add(&auto_paste_row);

        page.add(&security_group);
        dialog.add(&page);

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
            if let Ok(mut client) = IpcClient::connect().await {
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
