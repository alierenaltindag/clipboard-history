#[cfg(feature = "gtk")]
use clipboard_history_core::config::AppConfig;
#[cfg(feature = "gtk")]
use gtk4::prelude::*;
#[cfg(feature = "gtk")]
use libadwaita::prelude::*;

#[cfg(feature = "gtk")]
pub struct SettingsDialog;

#[cfg(feature = "gtk")]
impl SettingsDialog {
    pub fn show(parent: &gtk4::Window, config: AppConfig) {
        let dialog = adw::PreferencesWindow::new();
        dialog.set_transient_for(Some(parent));
        dialog.set_modal(true);
        dialog.set_title(Some("Clipboard Preferences"));

        let page = adw::PreferencesPage::new();
        page.set_title("General");
        page.set_icon_name(Some("preferences-system-symbolic"));

        // General Group
        let general_group = adw::PreferencesGroup::new();
        general_group.set_title("Retention & Capacity");

        let max_entries_row = adw::SpinRow::new(
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

        let retention_row = adw::SpinRow::new(
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

        // Security Group
        let security_group = adw::PreferencesGroup::new();
        security_group.set_title("Privacy & Security");

        let pwd_row = adw::SwitchRow::new();
        pwd_row.set_title("Protect Password Managers");
        pwd_row.set_subtitle("Ignore copies from KeePassXC, 1Password, and Bitwarden");
        pwd_row.set_active(config.security.ignore_password_managers);
        security_group.add(&pwd_row);

        let auto_paste_row = adw::SwitchRow::new();
        auto_paste_row.set_title("Direct Keystroke Paste");
        auto_paste_row.set_subtitle("Automatically synthesize Ctrl+V when an entry is selected");
        auto_paste_row.set_active(config.paste.auto_paste);
        security_group.add(&auto_paste_row);

        page.add(&security_group);

        dialog.add(&page);
        dialog.present();
    }
}
