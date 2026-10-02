use clap::{Parser, Subcommand};
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::EntryType;
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use std::process::Command;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[command(
    name = "clipboard-history",
    author,
    version,
    about = "Linux Universal Clipboard History Manager CLI (Win+V alternative)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Toggle the popup GUI window (invokes clipboard-history-gui)")]
    Toggle,

    #[command(about = "List clipboard history items")]
    List {
        #[arg(
            short,
            long,
            default_value = "20",
            help = "Max number of items to display"
        )]
        limit: usize,

        #[arg(short, long, default_value = "0", help = "Offset")]
        offset: usize,

        #[arg(
            short,
            long,
            help = "Filter by type (text, html, image, urilist, code)"
        )]
        r#type: Option<String>,

        #[arg(short, long, help = "Show only pinned items")]
        pinned: bool,
    },

    #[command(about = "Search clipboard history")]
    Search {
        #[arg(help = "Search query string")]
        query: String,

        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    #[command(about = "Pin an entry by ID to prevent automatic eviction")]
    Pin {
        #[arg(help = "Entry ID")]
        id: String,
    },

    #[command(about = "Unpin an entry by ID")]
    Unpin {
        #[arg(help = "Entry ID")]
        id: String,
    },

    #[command(about = "Delete an entry by ID")]
    Delete {
        #[arg(help = "Entry ID")]
        id: String,
    },

    #[command(about = "Clear clipboard history")]
    Clear {
        #[arg(short, long, help = "Delete all entries including pinned")]
        all: bool,
    },

    #[command(about = "Pause clipboard tracking")]
    Pause,

    #[command(about = "Resume clipboard tracking")]
    Resume,

    #[command(about = "Display daemon status and metrics")]
    Status,

    #[command(about = "Add a text entry to clipboard history manually")]
    Add {
        #[arg(help = "Text to add")]
        text: String,
    },

    #[command(about = "Manage clipboard history configuration settings")]
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    #[command(about = "Display current configuration")]
    Show,

    #[command(about = "Toggle protection for password managers (KeePassXC, 1Password, Bitwarden)")]
    SetPasswords {
        #[arg(action = clap::ArgAction::Set, help = "true to protect (do not save), false to allow saving")]
        enabled: bool,
    },

    #[command(about = "Toggle protection for incognito and private browsing windows")]
    SetIncognito {
        #[arg(action = clap::ArgAction::Set, help = "true to ignore (do not save), false to allow saving")]
        enabled: bool,
    },

    #[command(about = "Toggle direct keystroke auto-paste")]
    SetAutoPaste {
        #[arg(action = clap::ArgAction::Set, help = "true to enable auto-paste, false to copy-only")]
        enabled: bool,
    },

    #[command(about = "Set maximum entries limit")]
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let socket_path = AppConfig::socket_path();

    match cli.command {
        Commands::Toggle => {
            // First check if daemon socket exists or try sending request
            let toggle_req = IpcRequest::ToggleWindow;
            let result = IpcClient::send_request(&socket_path, &toggle_req).await;
            if result.is_err() {
                // Spawn GUI process directly
                let _ = Command::new("clipboard-history-gui")
                    .arg("--toggle")
                    .spawn();
            }
            println!("Triggered clipboard history window toggle.");
        }

        Commands::List {
            limit,
            offset,
            r#type,
            pinned,
        } => {
            let filter = r#type.and_then(|t| EntryType::from_str(&t).ok());
            let req = IpcRequest::GetEntries {
                limit,
                offset,
                filter,
                pinned_only: pinned,
            };

            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Entries(entries)) => {
                    if entries.is_empty() {
                        println!("No clipboard entries found.");
                        return Ok(());
                    }

                    println!(
                        "{:<8} | {:<7} | {:<4} | {:<50} | {:<16}",
                        "ID", "TYPE", "PIN", "PREVIEW", "SIZE"
                    );
                    println!("{:-<95}", "");

                    for entry in entries {
                        let short_id = &entry.id[..8.min(entry.id.len())];
                        let pin_mark = if entry.is_pinned { "📌" } else { "  " };
                        let preview_cut = if entry.preview.chars().count() > 47 {
                            format!("{}...", entry.preview.chars().take(44).collect::<String>())
                        } else {
                            entry.preview.clone()
                        };
                        let size_str = format!("{:.1} KB", entry.size_bytes as f64 / 1024.0);

                        println!(
                            "{:<8} | {:<7} | {:<4} | {:<50} | {:<16}",
                            short_id,
                            entry.entry_type.to_string(),
                            pin_mark,
                            preview_cut,
                            size_str
                        );
                    }
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response from daemon"),
            }
        }

        Commands::Search { query, limit } => {
            let req = IpcRequest::Search {
                query,
                limit,
                offset: 0,
            };

            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Entries(entries)) => {
                    if entries.is_empty() {
                        println!("No matching entries found.");
                        return Ok(());
                    }

                    println!(
                        "{:<8} | {:<7} | {:<4} | {:<60}",
                        "ID", "TYPE", "PIN", "PREVIEW"
                    );
                    println!("{:-<85}", "");

                    for entry in entries {
                        let short_id = &entry.id[..8.min(entry.id.len())];
                        let pin_mark = if entry.is_pinned { "📌" } else { "  " };
                        println!(
                            "{:<8} | {:<7} | {:<4} | {:<60}",
                            short_id,
                            entry.entry_type.to_string(),
                            pin_mark,
                            entry.preview
                        );
                    }
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Pin { id } => {
            let req = IpcRequest::PinEntry { id: id.clone() };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Success) => println!("Pinned entry {}", id),
                Ok(IpcResponse::Error(e)) => eprintln!("Failed to pin entry: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Unpin { id } => {
            let req = IpcRequest::UnpinEntry { id: id.clone() };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Success) => println!("Unpinned entry {}", id),
                Ok(IpcResponse::Error(e)) => eprintln!("Failed to unpin entry: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Delete { id } => {
            let req = IpcRequest::DeleteEntry { id: id.clone() };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Success) => println!("Deleted entry {}", id),
                Ok(IpcResponse::Error(e)) => eprintln!("Failed to delete entry: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Clear { all } => {
            let req = IpcRequest::ClearHistory {
                include_pinned: all,
            };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Success) => {
                    if all {
                        println!("Cleared all clipboard history (including pinned items).");
                    } else {
                        println!("Cleared unpinned clipboard history (pinned items preserved).");
                    }
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Failed to clear history: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Pause => {
            let req = IpcRequest::SetPause { paused: true };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Success) => println!("Clipboard tracking PAUSED."),
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Resume => {
            let req = IpcRequest::SetPause { paused: false };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Success) => println!("Clipboard tracking RESUMED."),
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Status => {
            let req = IpcRequest::GetStatus;
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Status(status)) => {
                    let uptime_min = status.uptime_secs / 60;
                    let uptime_hours = uptime_min / 60;
                    let uptime_display = if uptime_hours > 0 {
                        format!("{}h {}m", uptime_hours, uptime_min % 60)
                    } else {
                        format!("{}m {}s", uptime_min, status.uptime_secs % 60)
                    };

                    println!("Clipboard History Daemon Status");
                    println!("------------------------------");
                    println!("Daemon Service : ONLINE");
                    println!("Uptime         : {}", uptime_display);
                    println!(
                        "Monitoring     : {}",
                        if status.is_paused { "PAUSED" } else { "ACTIVE" }
                    );
                    println!("Total Entries  : {}", status.total_entries);
                    println!(
                        "Database Size  : {:.2} KB",
                        status.db_size_bytes as f64 / 1024.0
                    );
                    println!("Socket Path    : {}", socket_path.display());
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Add { text } => {
            let req = IpcRequest::AddManualEntry { text };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::Success) => println!("Added entry to clipboard history."),
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Config { action } => {
            // Retrieve current configuration: try daemon IPC first, fallback to disk
            let mut cfg = match IpcClient::send_request(&socket_path, &IpcRequest::GetConfig).await
            {
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
                }
                ConfigAction::SetPasswords { enabled } => {
                    cfg.security.ignore_password_managers = enabled;
                    save_and_broadcast_config(&socket_path, &cfg).await?;
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
                    save_and_broadcast_config(&socket_path, &cfg).await?;
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
                ConfigAction::SetAutoPaste { enabled } => {
                    cfg.paste.auto_paste = enabled;
                    save_and_broadcast_config(&socket_path, &cfg).await?;
                    println!(
                        "Direct keystroke auto-paste updated: {}",
                        if enabled { "ENABLED" } else { "DISABLED" }
                    );
                }
                ConfigAction::SetMaxEntries { limit } => {
                    cfg.general.max_entries = limit;
                    save_and_broadcast_config(&socket_path, &cfg).await?;
                    println!("Maximum history entries limit updated to: {}", limit);
                }
                ConfigAction::SetRetention { days } => {
                    cfg.general.retention_days = days;
                    save_and_broadcast_config(&socket_path, &cfg).await?;
                    println!("Retention duration updated to: {} days", days);
                }
            }
        }
    }

    Ok(())
}

async fn save_and_broadcast_config(
    socket_path: &std::path::Path,
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

fn print_daemon_offline_error() {
    eprintln!("Error: Could not connect to clipboard-history-daemon.");
    eprintln!("Ensure the daemon is running:");
    eprintln!("  systemctl --user start clipboard-history");
    eprintln!("  or run: clipboard-history-daemon &");
}
