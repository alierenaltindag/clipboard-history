use crate::error::{CoreError, Result};
use crate::security::SecretHandlingPolicy;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub max_entries: usize,
    pub retention_days: u32,
    pub max_blob_size_mb: usize,
    pub poll_interval_ms: u64,
    pub rate_limit_per_second: u32,
    #[serde(default = "default_capture_primary")]
    pub capture_primary_selection: bool,
    #[serde(default = "default_primary_debounce")]
    pub primary_debounce_ms: u64,
    #[serde(default = "default_primary_min_len")]
    pub primary_min_length: usize,
}

fn default_capture_primary() -> bool {
    true
}
fn default_primary_debounce() -> u64 {
    300
}
fn default_primary_min_len() -> usize {
    2
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            max_entries: 500,
            retention_days: 30,
            max_blob_size_mb: 50,
            poll_interval_ms: 300,
            rate_limit_per_second: 10,
            capture_primary_selection: true,
            primary_debounce_ms: 300,
            primary_min_length: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppFilterMode {
    #[default]
    Blacklist,
    Whitelist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub ignore_password_managers: bool,
    pub ignore_incognito_windows: bool,
    pub secret_policy: SecretHandlingPolicy,
    pub custom_secret_patterns: Vec<String>,
    pub ignored_window_classes: Vec<String>,
    pub encryption_enabled: bool,
    pub incognito_window_patterns: Vec<String>,
    #[serde(default)]
    pub app_filter_mode: AppFilterMode,
    #[serde(default)]
    pub app_filter_list: Vec<String>,
    #[serde(default)]
    pub auto_clean_tracking_urls: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            ignore_password_managers: true,
            ignore_incognito_windows: true,
            secret_policy: SecretHandlingPolicy::Reject,
            custom_secret_patterns: Vec::new(),
            ignored_window_classes: vec![
                "keepassxc".to_string(),
                "1password".to_string(),
                "bitwarden".to_string(),
            ],
            encryption_enabled: false,
            incognito_window_patterns: vec![
                "*incognito*".to_string(),
                "*private browsing*".to_string(),
                "*tor browser*".to_string(),
                "*keepass*".to_string(),
                "*1password*".to_string(),
                "*bitwarden*".to_string(),
            ],
            app_filter_mode: AppFilterMode::Blacklist,
            app_filter_list: Vec::new(),
            auto_clean_tracking_urls: false,
        }
    }
}

impl SecurityConfig {
    /// Determines whether a clipboard copy from the given app and/or window class is permitted.
    pub fn is_app_allowed(&self, app_name: Option<&str>, window_class: Option<&str>) -> bool {
        let mut candidates = Vec::new();
        if let Some(app) = app_name {
            let trimmed = app.trim();
            if !trimmed.is_empty() {
                candidates.push(trimmed.to_lowercase());
            }
        }
        if let Some(cls) = window_class {
            let trimmed = cls.trim();
            if !trimmed.is_empty() {
                candidates.push(trimmed.to_lowercase());
            }
        }

        let matches_pattern = |pattern: &str, candidate: &str| -> bool {
            let clean = pattern.trim().trim_matches('*').to_lowercase();
            !clean.is_empty() && candidate.contains(&clean)
        };

        match self.app_filter_mode {
            AppFilterMode::Blacklist => {
                // If candidate matches any item in app_filter_list or legacy ignored_window_classes, reject
                for candidate in &candidates {
                    for filter in &self.app_filter_list {
                        if matches_pattern(filter, candidate) {
                            return false;
                        }
                    }
                    for legacy in &self.ignored_window_classes {
                        if matches_pattern(legacy, candidate) {
                            return false;
                        }
                    }
                }
                true
            }
            AppFilterMode::Whitelist => {
                // In whitelist mode, if list is empty, allow everything
                if self.app_filter_list.is_empty() {
                    return true;
                }
                // If candidates is empty (unknown origin), do not allow
                if candidates.is_empty() {
                    return false;
                }
                // Check if any candidate matches any pattern in app_filter_list
                for candidate in &candidates {
                    for filter in &self.app_filter_list {
                        if matches_pattern(filter, candidate) {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
    pub window_width: u32,
    pub window_height: u32,
    pub font_size: Option<f64>,
    pub show_preview_thumbnails: bool,
    pub tray_icon_enabled: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            window_width: 330,
            window_height: 520,
            font_size: None,
            show_preview_thumbnails: true,
            tray_icon_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PasteConfig {
    pub auto_paste: bool,
    pub paste_delay_ms: u64,
    pub preferred_injector: Option<String>,
    pub move_pasted_to_top: bool,
}

impl Default for PasteConfig {
    fn default() -> Self {
        Self {
            auto_paste: true,
            paste_delay_ms: 120,
            preferred_injector: None,
            move_pasted_to_top: false,
        }
    }
}

impl PasteConfig {
    pub fn effective_delay_ms(&self) -> u64 {
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland";
        let min_delay = if is_wayland { 120 } else { 50 };
        self.paste_delay_ms.max(min_delay)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub shortcut: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            shortcut: "Super+C".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub enabled: bool,
    pub device_name: String,
    pub listen_port: u16,
    pub pairing_pin: String,
    pub peer_addresses: Vec<String>,
    #[serde(default)]
    pub trusted_peers: Vec<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "Linux-Desktop".to_string());
        Self {
            enabled: false,
            device_name: hostname,
            listen_port: 54123,
            pairing_pin: "123456".to_string(),
            peer_addresses: Vec::new(),
            trusted_peers: Vec::new(),
        }
    }
}

impl SyncConfig {
    pub fn is_peer_trusted(&self, peer_addr: &str, device_name: &str) -> bool {
        let clean_ip = if let Some(idx) = peer_addr.find(':') {
            &peer_addr[..idx]
        } else {
            peer_addr
        };

        self.trusted_peers.iter().any(|t| {
            t == peer_addr
                || t == clean_ip
                || (!t.is_empty() && peer_addr.starts_with(t))
                || (!device_name.is_empty() && t == device_name)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    pub auto_check: bool,
    pub check_interval_hours: u64,
    pub last_check_timestamp: u64,
    pub last_notified_version: Option<String>,
    pub ignored_version: Option<String>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_check: true,
            check_interval_hours: 24,
            last_check_timestamp: 0,
            last_notified_version: None,
            ignored_version: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub security: SecurityConfig,
    pub ui: UiConfig,
    pub paste: PasteConfig,
    pub hotkey: HotkeyConfig,
    pub sync: SyncConfig,
    #[serde(default)]
    pub updates: UpdateConfig,
}

impl AppConfig {
    pub fn project_dirs() -> Option<ProjectDirs> {
        ProjectDirs::from("com", "antigravity", "clipboard-history")
    }

    pub fn config_dir() -> PathBuf {
        if let Some(dirs) = Self::project_dirs() {
            dirs.config_dir().to_path_buf()
        } else if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg).join("clipboard-history")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home)
                .join(".config")
                .join("clipboard-history")
        } else {
            PathBuf::from("/tmp/clipboard-history/config")
        }
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn secret_key_path() -> PathBuf {
        Self::config_dir().join("secret.key")
    }

    pub fn data_dir() -> PathBuf {
        if let Some(dirs) = Self::project_dirs() {
            dirs.data_dir().to_path_buf()
        } else if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            PathBuf::from(xdg).join("clipboard-history")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("clipboard-history")
        } else {
            PathBuf::from("/tmp/clipboard-history/data")
        }
    }

    pub fn db_path() -> PathBuf {
        Self::data_dir().join("history.db")
    }

    pub fn blobs_dir() -> PathBuf {
        Self::data_dir().join("blobs")
    }

    pub fn socket_path() -> PathBuf {
        // Prefer XDG_RUNTIME_DIR for Unix sockets if available, fallback to data_dir
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            PathBuf::from(runtime_dir).join("clipboard-history.sock")
        } else {
            Self::data_dir().join("daemon.sock")
        }
    }

    pub fn auth_token_path() -> PathBuf {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            PathBuf::from(runtime_dir).join("clipboard-history.token")
        } else {
            Self::config_dir().join("auth_token")
        }
    }

    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str::<AppConfig>(&content) {
                    Ok(cfg) => {
                        info!("Loaded configuration from {}", path.display());
                        return cfg;
                    }
                    Err(e) => {
                        warn!("Failed to parse config file: {}. Using defaults.", e);
                    }
                },
                Err(e) => {
                    warn!("Failed to read config file: {}. Using defaults.", e);
                }
            }
        }

        let default_config = Self::default();
        if let Err(e) = default_config.save(&path) {
            warn!("Could not save default config to {}: {}", path.display(), e);
        }
        default_config
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }

        let serialized =
            toml::to_string_pretty(self).map_err(|e| CoreError::Config(e.to_string()))?;
        fs::write(path, serialized)?;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        info!("Saved configuration to {}", path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_delay_ms_minimums() {
        let mut cfg = PasteConfig {
            auto_paste: true,
            paste_delay_ms: 20,
            preferred_injector: None,
            move_pasted_to_top: false,
        };

        let delay = cfg.effective_delay_ms();
        // Since we are running on Linux (either Wayland or X11/headless), delay should be at least 50ms,
        // and if Wayland is set, at least 120ms.
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland";
        let expected_min = if is_wayland { 120 } else { 50 };
        assert_eq!(delay, expected_min);

        // High delay should be preserved
        cfg.paste_delay_ms = 350;
        assert_eq!(cfg.effective_delay_ms(), 350);
    }
}
