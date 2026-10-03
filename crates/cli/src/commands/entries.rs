use clipboard_history_core::domain::EntryType;
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use std::path::Path;
use std::str::FromStr;

pub async fn handle_list(
    socket_path: &Path,
    limit: usize,
    offset: usize,
    r#type: Option<String>,
    pinned: bool,
) {
    let filter = r#type.and_then(|t| EntryType::from_str(&t).ok());
    let req = IpcRequest::GetEntries {
        limit,
        offset,
        filter,
        pinned_only: pinned,
    };

    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Entries(entries)) => {
            if entries.is_empty() {
                println!("No clipboard entries found.");
                return;
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
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response from daemon"),
    }
}

pub async fn handle_search(socket_path: &Path, query: String, limit: usize) {
    let req = IpcRequest::Search {
        query,
        limit,
        offset: 0,
    };

    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Entries(entries)) => {
            if entries.is_empty() {
                println!("No matching entries found.");
                return;
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
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_pin(socket_path: &Path, id: String) {
    let req = IpcRequest::PinEntry { id: id.clone() };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Success) => println!("Pinned entry {}", id),
        Ok(IpcResponse::Error(e)) => eprintln!("Failed to pin entry: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_unpin(socket_path: &Path, id: String) {
    let req = IpcRequest::UnpinEntry { id: id.clone() };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Success) => println!("Unpinned entry {}", id),
        Ok(IpcResponse::Error(e)) => eprintln!("Failed to unpin entry: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_delete(socket_path: &Path, id: String) {
    let req = IpcRequest::DeleteEntry { id: id.clone() };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Success) => println!("Deleted entry {}", id),
        Ok(IpcResponse::Error(e)) => eprintln!("Failed to delete entry: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_clear(socket_path: &Path, all: bool) {
    let req = IpcRequest::ClearHistory {
        include_pinned: all,
    };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Success) => {
            if all {
                println!("Cleared all clipboard history (including pinned items).");
            } else {
                println!("Cleared unpinned clipboard history (pinned items preserved).");
            }
        }
        Ok(IpcResponse::Error(e)) => eprintln!("Failed to clear history: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_batch_delete(socket_path: &Path, ids: Vec<String>) {
    let count = ids.len();
    let req = IpcRequest::BatchDelete { ids };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::BatchSuccess { count: deleted }) => {
            println!(
                "Successfully deleted {} entries (requested {}).",
                deleted, count
            );
        }
        Ok(IpcResponse::Error(e)) => eprintln!("Batch Delete Error: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_batch_pin(socket_path: &Path, unpin: bool, ids: Vec<String>) {
    let count = ids.len();
    let pin_flag = !unpin;
    let req = IpcRequest::BatchPin {
        ids,
        pinned: pin_flag,
    };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::BatchSuccess { count: updated }) => {
            println!(
                "Successfully {} {} entries (requested {}).",
                if pin_flag { "pinned" } else { "unpinned" },
                updated,
                count
            );
        }
        Ok(IpcResponse::Error(e)) => eprintln!("Batch Pin Error: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_add(socket_path: &Path, text: String) {
    let req = IpcRequest::AddManualEntry { text };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Success) => println!("Added entry to clipboard history."),
        Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}
