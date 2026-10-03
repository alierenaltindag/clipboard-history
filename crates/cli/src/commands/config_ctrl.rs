use clipboard_history_core::config::{AppConfig, AppFilterMode};
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use std::path::Path;

#[derive(clap::Subcommand, Debug)]
pub enum ConfigAction {
    #[command(about = "Display current configuration summary")]
    Show,
    #[command(about = "Enable or disable password manager detection")]
    SetPasswords {
        #[arg(action = clap::ArgAction::Set, help = "true to ignore password managers, false to capture")]
        enabled: bool,
    },
    #[command(about = "Enable or disable incognito/private browsing window detection")]
    SetIncognito {
        #[arg(action = clap::ArgAction::Set, help = "true to ignore incognito copies, false to capture")]
        enabled: bool,
    },
    #[command(about = "Enable or disable automatic tracking parameter cleaning from copied URLs")]
    SetCleanUrls {
        #[arg(action = clap::ArgAction::Set, help = "true to clean URLs, false to keep raw URLs")]
        enabled: bool,
    },
    #[command(about = "Set application filtering mode (blacklist or whitelist)")]
    SetAppFilterMode {
        #[arg(help = "Filter mode: 'blacklist' or 'whitelist'")]
        mode: String,
    },
    #[command(about = "Add an application window class or title substring to the filter list")]
    AddAppFilter {
        #[arg(help = "App process name, WM_CLASS, or title substring")]
        app: String,
    },
    #[command(about = "Remove an application from the filter list")]
    RemoveAppFilter {
        #[arg(help = "App name to remove from filter list")]
        app: String,
    },
    #[command(about = "Toggle direct keystroke auto-paste")]
    SetAutoPaste {
        #[arg(action = clap::ArgAction::Set, help = "true to enable auto-paste, false to copy-only")]
        enabled: bool,
    },
    #[command(about = "Toggle encrypted P2P LAN synchronization")]
    SetSync {
        #[arg(action = clap::ArgAction::Set, help = "true to enable sync, false to disable")]
        enabled: bool,
    },
    #[command(about = "Set the 6-digit cryptographic pairing PIN for LAN sync")]
    SetPin {
        #[arg(help = "6-digit pairing PIN")]
        pin: String,
    },
    #[command(about = "Set maximum history capacity limit")]
    SetMaxEntries {
        #[arg(help = "Maximum number of history entries")]
        limit: usize,
    },
    #[command(about = "Set retention duration in days")]
    SetRetention {
        #[arg(help = "Retention period in days")]
        days: u32,
    },
}

pub async fn handle_config(
    socket_path: &Path,
    action: Option<ConfigAction>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Retrieve current configuration: try daemon IPC first, fallback to disk
    let mut cfg = match IpcClient::send_request(socket_path, &IpcRequest::GetConfig).await {
        Ok(IpcResponse::Config(c)) => *c,
        _ => AppConfig::load_or_default(),
    };

    match action.unwrap_or(ConfigAction::Show) {
        ConfigAction::Show => {
            println!("Clipboard History Configuration");
            println!("===============================");
            println!(
                "Config File               : {}",
                AppConfig::config_path().display()
            );
            println!();
            println!("Security & Privacy:");
            println!(
                "  Protect Password Managers : {} (ignore KeePassXC, Bitwarden, etc.)",
                if cfg.security.ignore_password_managers {
                    "ENABLED [Default]"
                } else {
                    "DISABLED"
                }
            );
            println!(
                "  Ignore Incognito Windows  : {} (ignore private browsing windows)",
                if cfg.security.ignore_incognito_windows {
                    "ENABLED [Default]"
                } else {
                    "DISABLED"
                }
            );
            println!(
                "  Auto-clean Tracking URLs  : {} (strip utm_*, fbclid, gclid, etc.)",
                if cfg.security.auto_clean_tracking_urls {
                    "ENABLED"
                } else {
                    "DISABLED"
                }
            );
            println!(
                "  App Filter Mode           : {:?}",
                cfg.security.app_filter_mode
            );
            println!(
                "  App Filter List           : {:?}",
                cfg.security.app_filter_list
            );
            println!(
                "  Secret Handling Policy   : {:?}",
                cfg.security.secret_policy
            );
            println!(
                "  Storage Encryption       : {}",
                if cfg.security.encryption_enabled {
                    "ENABLED (AES-256-GCM)"
                } else {
                    "DISABLED"
                }
            );
            println!();
            println!("General & Retention:");
            println!("  Max History Entries       : {}", cfg.general.max_entries);
            println!(
                "  Retention Period (Days)   : {}",
                cfg.general.retention_days
            );
            println!(
                "  Max Blob Size (MB)        : {}",
                cfg.general.max_blob_size_mb
            );
            println!();
            println!("Paste Behavior:");
            println!(
                "  Direct Keystroke Paste    : {}",
                if cfg.paste.auto_paste {
                    "ENABLED (Ctrl+V)"
                } else {
                    "DISABLED (Copy only)"
                }
            );
            println!("  Paste Delay (ms)          : {}", cfg.paste.paste_delay_ms);
            println!();
            println!("UI & System:");
            println!("  Hotkey Shortcut           : {}", cfg.hotkey.shortcut);
            println!("  Theme                     : {}", cfg.ui.theme);
            println!(
                "  System Tray Icon          : {}",
                if cfg.ui.tray_icon_enabled {
                    "ENABLED"
                } else {
                    "DISABLED"
                }
            );
            println!();
            println!("P2P Local Network Sync:");
            println!(
                "  LAN Sync Status           : {}",
                if cfg.sync.enabled {
                    "ENABLED"
                } else {
                    "DISABLED"
                }
            );
            println!("  Device Name               : {}", cfg.sync.device_name);
            println!("  Listen Port               : {}", cfg.sync.listen_port);
            println!("  Pairing PIN               : {}", cfg.sync.pairing_pin);
        }
        ConfigAction::SetPasswords { enabled } => {
            cfg.security.ignore_password_managers = enabled;
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!(
                "Password manager protection updated: {} ({})",
                if enabled { "ENABLED" } else { "DISABLED" },
                if enabled {
                    "copies from KeePassXC/Bitwarden/1Password will be ignored"
                } else {
                    "copies from password managers will be saved"
                }
            );
        }
        ConfigAction::SetIncognito { enabled } => {
            cfg.security.ignore_incognito_windows = enabled;
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!(
                "Incognito window protection updated: {} ({})",
                if enabled { "ENABLED" } else { "DISABLED" },
                if enabled {
                    "copies from incognito/private browsing windows will be ignored"
                } else {
                    "copies from incognito/private windows will be saved"
                }
            );
        }
        ConfigAction::SetCleanUrls { enabled } => {
            cfg.security.auto_clean_tracking_urls = enabled;
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!(
                "Auto-clean tracking URLs updated: {} ({})",
                if enabled { "ENABLED" } else { "DISABLED" },
                if enabled {
                    "tracking parameters (utm_*, fbclid, etc.) will be automatically removed from copied links"
                } else {
                    "raw copied links will be preserved unchanged"
                }
            );
        }
        ConfigAction::SetAppFilterMode { mode } => {
            let new_mode = match mode.to_lowercase().as_str() {
                "blacklist" | "black" => AppFilterMode::Blacklist,
                "whitelist" | "white" => AppFilterMode::Whitelist,
                other => {
                    eprintln!(
                        "Invalid filter mode '{}'. Expected 'blacklist' or 'whitelist'.",
                        other
                    );
                    return Ok(());
                }
            };
            cfg.security.app_filter_mode = new_mode;
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!("Application filter mode updated to: {:?}", new_mode);
        }
        ConfigAction::AddAppFilter { app } => {
            let trimmed = app.trim().to_string();
            if !trimmed.is_empty() && !cfg.security.app_filter_list.contains(&trimmed) {
                cfg.security.app_filter_list.push(trimmed.clone());
                save_and_broadcast_config(socket_path, &cfg).await?;
                println!(
                    "Added '{}' to application filter list: {:?}",
                    trimmed, cfg.security.app_filter_list
                );
            } else {
                println!(
                    "'{}' is already in application filter list: {:?}",
                    trimmed, cfg.security.app_filter_list
                );
            }
        }
        ConfigAction::RemoveAppFilter { app } => {
            let trimmed = app.trim().to_lowercase();
            let before = cfg.security.app_filter_list.len();
            cfg.security
                .app_filter_list
                .retain(|a| a.trim().to_lowercase() != trimmed);
            if cfg.security.app_filter_list.len() < before {
                save_and_broadcast_config(socket_path, &cfg).await?;
                println!(
                    "Removed '{}' from application filter list: {:?}",
                    app, cfg.security.app_filter_list
                );
            } else {
                println!(
                    "'{}' was not found in application filter list: {:?}",
                    app, cfg.security.app_filter_list
                );
            }
        }
        ConfigAction::SetAutoPaste { enabled } => {
            cfg.paste.auto_paste = enabled;
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!(
                "Direct keystroke auto-paste updated: {}",
                if enabled { "ENABLED" } else { "DISABLED" }
            );
        }
        ConfigAction::SetSync { enabled } => {
            cfg.sync.enabled = enabled;
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!(
                "P2P LAN Sync updated: {}",
                if enabled { "ENABLED" } else { "DISABLED" }
            );
        }
        ConfigAction::SetPin { pin } => {
            cfg.sync.pairing_pin = pin.clone();
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!("P2P LAN sync pairing PIN updated to: {}", pin);
        }
        ConfigAction::SetMaxEntries { limit } => {
            cfg.general.max_entries = limit;
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!("Maximum history entries limit updated to: {}", limit);
        }
        ConfigAction::SetRetention { days } => {
            cfg.general.retention_days = days;
            save_and_broadcast_config(socket_path, &cfg).await?;
            println!("Retention duration updated to: {} days", days);
        }
    }

    Ok(())
}

async fn save_and_broadcast_config(
    socket_path: &Path,
    cfg: &AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    cfg.save(AppConfig::config_path())?;
    let req = IpcRequest::UpdateConfig {
        config: Box::new(cfg.clone()),
    };
    if let Ok(IpcResponse::Error(e)) = IpcClient::send_request(socket_path, &req).await {
        eprintln!("Warning: Daemon reported error applying config: {}", e);
    }
    Ok(())
}
