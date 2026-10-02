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

    #[command(about = "Manage permanent canned snippets")]
    Snippets {
        #[command(subcommand)]
        action: Option<SnippetAction>,
    },

    #[command(about = "Manage sequential paste queue")]
    Queue {
        #[command(subcommand)]
        action: Option<QueueAction>,
    },

    #[command(about = "Extract text from an image entry using OCR")]
    Ocr {
        #[arg(help = "Entry ID or blob hash")]
        id: String,
    },

    #[command(about = "Concatenate multiple clipboard entries and copy to clipboard")]
    Join {
        #[arg(short, long, help = "Custom delimiter (default: newline)")]
        delimiter: Option<String>,
        #[arg(short, long, help = "Format as numbered list (1. ..., 2. ...)")]
        numbered: bool,
        #[arg(short, long, help = "Format as bulleted list (- ..., - ...)")]
        bullets: bool,
        #[arg(help = "Entry IDs to concatenate in order")]
        ids: Vec<String>,
    },

    #[command(about = "Compare and display visual diff between two entries")]
    Diff {
        #[arg(help = "First entry ID (Original / A)")]
        id_a: String,
        #[arg(help = "Second entry ID (Modified / B)")]
        id_b: String,
        #[arg(long, help = "Disable ANSI color formatting in output")]
        no_color: bool,
    },

    #[command(about = "Delete multiple entries by ID in batch")]
    BatchDelete {
        #[arg(help = "Entry IDs to delete")]
        ids: Vec<String>,
    },

    #[command(about = "Pin or unpin multiple entries in batch")]
    BatchPin {
        #[arg(long, help = "Unpin instead of pin")]
        unpin: bool,
        #[arg(help = "Entry IDs to pin/unpin")]
        ids: Vec<String>,
    },

    #[command(about = "Clean tracking parameters from a URL or clipboard entry")]
    CleanUrl {
        #[arg(help = "URL string or clipboard entry ID")]
        target: String,
    },

    #[command(about = "Manage clipboard history configuration settings")]
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand, Debug)]
enum SnippetAction {
    #[command(about = "List all snippets")]
    List,
    #[command(about = "Add a new snippet")]
    Add {
        #[arg(help = "Snippet label/title")]
        label: String,
        #[arg(help = "Snippet content")]
        content: String,
        #[arg(short, long, help = "Optional category")]
        category: Option<String>,
    },
    #[command(about = "Delete a snippet by ID")]
    Delete {
        #[arg(help = "Snippet ID")]
        id: String,
    },
}

#[derive(Subcommand, Debug)]
enum QueueAction {
    #[command(about = "Display current paste queue status")]
    Status,
    #[command(about = "Enqueue entry IDs to the paste queue")]
    Add {
        #[arg(help = "Entry IDs to enqueue")]
        ids: Vec<String>,
    },
    #[command(about = "Pop and paste next item in queue")]
    Pop,
    #[command(about = "Clear the paste queue")]
    Clear,
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

    #[command(about = "Toggle automatic cleaning of tracking parameters from copied URLs")]
    SetCleanUrls {
        #[arg(action = clap::ArgAction::Set, help = "true to automatically clean tracking parameters, false to preserve raw URLs")]
        enabled: bool,
    },

    #[command(about = "Set application filter mode (blacklist or whitelist)")]
    SetAppFilterMode {
        #[arg(help = "Filter mode: blacklist or whitelist")]
        mode: String,
    },

    #[command(about = "Add application name or window class to filter list")]
    AddAppFilter {
        #[arg(help = "Application name or window class (e.g. 'slack', 'org.gnome.Calculator')")]
        app: String,
    },

    #[command(about = "Remove application name or window class from filter list")]
    RemoveAppFilter {
        #[arg(help = "Application name or window class to remove")]
        app: String,
    },

    #[command(about = "Toggle direct keystroke auto-paste")]
    SetAutoPaste {
        #[arg(action = clap::ArgAction::Set, help = "true to enable auto-paste, false to copy-only")]
        enabled: bool,
    },

    #[command(about = "Toggle P2P LAN clipboard synchronization")]
    SetSync {
        #[arg(action = clap::ArgAction::Set, help = "true to enable LAN sync, false to disable")]
        enabled: bool,
    },

    #[command(about = "Set P2P LAN sync pairing PIN")]
    SetPin {
        #[arg(help = "6-digit pairing PIN")]
        pin: String,
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

        Commands::Snippets { action } => match action.unwrap_or(SnippetAction::List) {
            SnippetAction::List => {
                let req = IpcRequest::ListSnippets { category: None };
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::Snippets(snippets)) => {
                        if snippets.is_empty() {
                            println!("No snippets saved yet. Add one with: clipboard-history snippets add <label> <content>");
                            return Ok(());
                        }
                        println!(
                            "{:<8} | {:<20} | {:<12} | {:<40}",
                            "ID", "LABEL", "CATEGORY", "CONTENT"
                        );
                        println!("{:-<85}", "");
                        for s in snippets {
                            let short_id = &s.id[..8.min(s.id.len())];
                            let preview = if s.content.len() > 37 {
                                format!("{}...", &s.content[..37])
                            } else {
                                s.content.clone()
                            };
                            println!(
                                "{:<8} | {:<20} | {:<12} | {:<40}",
                                short_id, s.label, s.category, preview
                            );
                        }
                    }
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                    Err(_) => print_daemon_offline_error(),
                    _ => eprintln!("Unexpected response"),
                }
            }
            SnippetAction::Add {
                label,
                content,
                category,
            } => {
                let req = IpcRequest::CreateSnippet {
                    label: label.clone(),
                    content,
                    category,
                };
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::Snippet(s)) => {
                        println!("Created snippet '{}' (ID: {})", s.label, s.id)
                    }
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                    Err(_) => print_daemon_offline_error(),
                    _ => eprintln!("Unexpected response"),
                }
            }
            SnippetAction::Delete { id } => {
                let req = IpcRequest::DeleteSnippet { id: id.clone() };
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::Success) => println!("Deleted snippet {}", id),
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                    Err(_) => print_daemon_offline_error(),
                    _ => eprintln!("Unexpected response"),
                }
            }
        },

        Commands::Queue { action } => match action.unwrap_or(QueueAction::Status) {
            QueueAction::Status => {
                let req = IpcRequest::GetQueueStatus;
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::QueueStatus(qs)) => {
                        println!("Sequential Paste Queue Status");
                        println!("----------------------------");
                        println!(
                            "Active          : {}",
                            if qs.active { "YES" } else { "EMPTY / IDLE" }
                        );
                        println!("Items Remaining : {}", qs.remaining_count);
                        if let Some(next) = qs.next_preview {
                            println!("Next in Line    : {}", next);
                        }
                    }
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                    Err(_) => print_daemon_offline_error(),
                    _ => eprintln!("Unexpected response"),
                }
            }
            QueueAction::Add { ids } => {
                let count = ids.len();
                let req = IpcRequest::EnqueueItems { ids };
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::Success) => {
                        println!("Enqueued {} item(s) to paste queue.", count)
                    }
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                    Err(_) => print_daemon_offline_error(),
                    _ => eprintln!("Unexpected response"),
                }
            }
            QueueAction::Pop => {
                let req = IpcRequest::PopAndPasteQueue;
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::QueuePopped {
                        remaining_count,
                        pasted,
                        text,
                    }) => {
                        if pasted {
                            if let Some(t) = text {
                                println!(
                                    "Popped item from queue ({} remaining):\n{}",
                                    remaining_count, t
                                );
                            } else {
                                println!("Popped item from queue ({} remaining).", remaining_count);
                            }
                        } else {
                            println!("Paste queue is empty.");
                        }
                    }
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                    Err(_) => print_daemon_offline_error(),
                    _ => eprintln!("Unexpected response"),
                }
            }
            QueueAction::Clear => {
                let req = IpcRequest::ClearQueue;
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::Success) => println!("Cleared paste queue."),
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                    Err(_) => print_daemon_offline_error(),
                    _ => eprintln!("Unexpected response"),
                }
            }
        },

        Commands::Ocr { id } => {
            // First try resolving entry by ID to get its blob_hash
            let blob_hash = match IpcClient::send_request(
                &socket_path,
                &IpcRequest::GetEntry { id: id.clone() },
            )
            .await
            {
                Ok(IpcResponse::Entry(entry)) => {
                    if let Some(e) = *entry {
                        e.blob_hash.unwrap_or(id.clone())
                    } else {
                        id.clone()
                    }
                }
                _ => id.clone(),
            };

            let req = IpcRequest::PerformOcr { blob_hash };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::OcrResult { text }) => {
                    println!("OCR Extracted Text:\n-------------------");
                    println!("{}", text);
                }
                Ok(IpcResponse::Error(e)) => eprintln!("OCR Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::Join {
            delimiter,
            numbered,
            bullets,
            ids,
        } => {
            if ids.is_empty() {
                eprintln!("Error: At least one entry ID must be provided to join.");
                return Ok(());
            }

            let mut texts = Vec::new();
            for id in &ids {
                let req = IpcRequest::GetEntry { id: id.clone() };
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::Entry(entry_opt)) => {
                        if let Some(entry) = *entry_opt {
                            texts.push(entry.text_content.unwrap_or(entry.preview));
                        } else {
                            eprintln!("Warning: Entry '{}' not found, skipping.", id);
                        }
                    }
                    _ => {
                        eprintln!("Warning: Could not fetch entry '{}', skipping.", id);
                    }
                }
            }

            if texts.is_empty() {
                eprintln!("Error: None of the specified entries could be retrieved.");
                return Ok(());
            }

            let delim = if numbered {
                clipboard_history_core::transforms::ConcatDelimiter::NumberedList
            } else if bullets {
                clipboard_history_core::transforms::ConcatDelimiter::BulletList
            } else if let Some(d) = delimiter {
                let unescaped = d.replace("\\n", "\n").replace("\\t", "\t");
                clipboard_history_core::transforms::ConcatDelimiter::Custom(unescaped)
            } else {
                clipboard_history_core::transforms::ConcatDelimiter::Newline
            };

            let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            let joined =
                clipboard_history_core::transforms::TextTransforms::concatenate(&text_refs, &delim);

            // Copy to daemon clipboard history
            let add_req = IpcRequest::AddManualEntry {
                text: joined.clone(),
            };
            let _ = IpcClient::send_request(&socket_path, &add_req).await;

            println!(
                "Concatenated {} items ({} bytes) into clipboard:",
                texts.len(),
                joined.len()
            );
            println!("{:-<50}", "");
            println!("{}", joined);
        }

        Commands::Diff {
            id_a,
            id_b,
            no_color,
        } => {
            let req = IpcRequest::ComputeDiff {
                id_a: id_a.clone(),
                id_b: id_b.clone(),
            };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::DiffResult(diff_res)) => {
                    if no_color {
                        println!("{}", diff_res.unified);
                    } else {
                        println!(
                            "\x1b[1mDiff Comparison\x1b[0m: \x1b[32m+{} additions\x1b[0m, \x1b[31m-{} deletions\x1b[0m",
                            diff_res.additions, diff_res.deletions
                        );
                        println!("{:-<50}", "");
                        for line in &diff_res.lines {
                            match line.tag {
                                clipboard_history_core::transforms::DiffTag::Insert => {
                                    println!("\x1b[32m+ {}\x1b[0m", line.text);
                                }
                                clipboard_history_core::transforms::DiffTag::Delete => {
                                    println!("\x1b[31m- {}\x1b[0m", line.text);
                                }
                                clipboard_history_core::transforms::DiffTag::Equal => {
                                    println!("  {}", line.text);
                                }
                            }
                        }
                    }
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Diff Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::BatchDelete { ids } => {
            let count = ids.len();
            let req = IpcRequest::BatchDelete { ids };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::BatchSuccess { count: deleted }) => {
                    println!(
                        "Successfully deleted {} entries (requested {}).",
                        deleted, count
                    );
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Batch Delete Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::BatchPin { unpin, ids } => {
            let count = ids.len();
            let pin_flag = !unpin;
            let req = IpcRequest::BatchPin {
                ids,
                pinned: pin_flag,
            };
            match IpcClient::send_request(&socket_path, &req).await {
                Ok(IpcResponse::BatchSuccess { count: updated }) => {
                    println!(
                        "Successfully {} {} entries (requested {}).",
                        if pin_flag { "pinned" } else { "unpinned" },
                        updated,
                        count
                    );
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Batch Pin Error: {}", e),
                Err(_) => print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }

        Commands::CleanUrl { target } => {
            let is_url = target.starts_with("http://") || target.starts_with("https://");
            let (original_text, entry_id) = if is_url {
                (target.clone(), None)
            } else {
                // Try fetching entry by ID from daemon
                let req = IpcRequest::GetEntry { id: target.clone() };
                match IpcClient::send_request(&socket_path, &req).await {
                    Ok(IpcResponse::Entry(entry_opt)) => {
                        if let Some(entry) = *entry_opt {
                            (
                                entry.text_content.unwrap_or(entry.preview),
                                Some(target.clone()),
                            )
                        } else {
                            (target.clone(), None)
                        }
                    }
                    _ => (target.clone(), None),
                }
            };

            let cleaned =
                clipboard_history_core::transforms::UrlCleaner::clean_text_urls(&original_text);

            if cleaned == original_text {
                println!("No tracking parameters detected in target:");
                println!("{}", original_text);
            } else {
                // Add cleaned entry to clipboard history
                let add_req = IpcRequest::AddManualEntry {
                    text: cleaned.clone(),
                };
                let _ = IpcClient::send_request(&socket_path, &add_req).await;

                if let Some(id) = entry_id {
                    println!("Cleaned tracking parameters from entry {}:", id);
                } else {
                    println!("Cleaned tracking parameters from URL:");
                }
                println!("Original : {}", original_text);
                println!("Cleaned  : {}", cleaned);
                println!("(Cleaned text copied to clipboard history)");
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
                ConfigAction::SetCleanUrls { enabled } => {
                    cfg.security.auto_clean_tracking_urls = enabled;
                    save_and_broadcast_config(&socket_path, &cfg).await?;
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
                        "blacklist" | "black" => {
                            clipboard_history_core::config::AppFilterMode::Blacklist
                        }
                        "whitelist" | "white" => {
                            clipboard_history_core::config::AppFilterMode::Whitelist
                        }
                        other => {
                            eprintln!(
                                "Invalid filter mode '{}'. Expected 'blacklist' or 'whitelist'.",
                                other
                            );
                            return Ok(());
                        }
                    };
                    cfg.security.app_filter_mode = new_mode;
                    save_and_broadcast_config(&socket_path, &cfg).await?;
                    println!("Application filter mode updated to: {:?}", new_mode);
                }
                ConfigAction::AddAppFilter { app } => {
                    let trimmed = app.trim().to_string();
                    if !trimmed.is_empty() && !cfg.security.app_filter_list.contains(&trimmed) {
                        cfg.security.app_filter_list.push(trimmed.clone());
                        save_and_broadcast_config(&socket_path, &cfg).await?;
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
                        save_and_broadcast_config(&socket_path, &cfg).await?;
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
                    save_and_broadcast_config(&socket_path, &cfg).await?;
                    println!(
                        "Direct keystroke auto-paste updated: {}",
                        if enabled { "ENABLED" } else { "DISABLED" }
                    );
                }
                ConfigAction::SetSync { enabled } => {
                    cfg.sync.enabled = enabled;
                    save_and_broadcast_config(&socket_path, &cfg).await?;
                    println!(
                        "P2P LAN Sync updated: {}",
                        if enabled { "ENABLED" } else { "DISABLED" }
                    );
                }
                ConfigAction::SetPin { pin } => {
                    cfg.sync.pairing_pin = pin.clone();
                    save_and_broadcast_config(&socket_path, &cfg).await?;
                    println!("P2P LAN sync pairing PIN updated to: {}", pin);
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
