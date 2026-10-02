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
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            max_entries: 500,
            retention_days: 30,
            max_blob_size_mb: 50,
            poll_interval_ms: 300,
            rate_limit_per_second: 10,
        }
    }
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
            window_width: 440,
            window_height: 580,
            font_size: None,
            show_preview_thumbnails: true,
            tray_icon_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasteConfig {
    pub auto_paste: bool,
    pub paste_delay_ms: u64,
    pub preferred_injector: Option<String>,
}

impl Default for PasteConfig {
    fn default() -> Self {
        Self {
            auto_paste: true,
            paste_delay_ms: 120,
            preferred_injector: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub shortcut: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            shortcut: "Super+V".to_string(),
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
}

impl AppConfig {
    pub fn project_dirs() -> ProjectDirs {
        ProjectDirs::from("com", "antigravity", "clipboard-history")
            .unwrap_or_else(|| panic!("Could not determine standard system directories"))
    }

    pub fn config_dir() -> PathBuf {
        Self::project_dirs().config_dir().to_path_buf()
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn secret_key_path() -> PathBuf {
        Self::config_dir().join("secret.key")
    }

    pub fn data_dir() -> PathBuf {
        Self::project_dirs().data_dir().to_path_buf()
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
