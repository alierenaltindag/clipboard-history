use crate::clipboard::copy_to_system_clipboard;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::injector::InjectorCascade;
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use std::path::Path;

#[derive(clap::Subcommand, Debug)]
pub enum QueueAction {
    #[command(about = "Display current paste queue status")]
    Status,
    #[command(about = "Enqueue entry IDs to the paste queue")]
    Add {
        #[arg(help = "Entry IDs to enqueue in order")]
        ids: Vec<String>,
    },
    #[command(about = "Pop and paste next item in queue")]
    Pop,
    #[command(about = "Clear the paste queue")]
    Clear,
}

pub async fn handle_queue(socket_path: &Path, action: Option<QueueAction>) {
    match action.unwrap_or(QueueAction::Status) {
        QueueAction::Status => {
            let req = IpcRequest::GetQueueStatus;
            match IpcClient::send_request(socket_path, &req).await {
                Ok(IpcResponse::QueueStatus(status)) => {
                    println!("Sequential Paste Queue Status");
                    println!("{:-<35}", "");
                    println!(
                        "  State           : {}",
                        if status.active { "ACTIVE" } else { "EMPTY" }
                    );
                    println!("  Remaining Items : {}", status.remaining_count);
                    if let Some(preview) = status.next_preview {
                        println!("  Next Item       : {}", preview);
                    }
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => super::print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }
        QueueAction::Add { ids } => {
            let count = ids.len();
            let req = IpcRequest::EnqueueItems { ids };
            match IpcClient::send_request(socket_path, &req).await {
                Ok(IpcResponse::Success) => {
                    println!("Enqueued {} item(s) to paste queue.", count)
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => super::print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }
        QueueAction::Pop => {
            let req = IpcRequest::PopAndPasteQueue;
            match IpcClient::send_request(socket_path, &req).await {
                Ok(IpcResponse::QueuePopped {
                    remaining_count,
                    pasted,
                    text,
                }) => {
                    if pasted {
                        if let Some(ref t) = text {
                            // Copy popped text to OS display server clipboard (MED-05)
                            copy_to_system_clipboard(t).await;
                            println!(
                                "Popped item from queue ({} remaining):\n{}",
                                remaining_count, t
                            );
                        } else {
                            println!("Popped item from queue ({} remaining).", remaining_count);
                        }

                        // Trigger synthetic keystroke paste if auto_paste is enabled (MED-14)
                        let cfg = AppConfig::load_or_default();
                        if cfg.paste.auto_paste {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                cfg.paste.paste_delay_ms,
                            ))
                            .await;
                            let cascade = InjectorCascade::new();
                            let (name, success) = cascade.execute_paste().await;
                            tracing::debug!("CLI auto-paste via {}: {}", name, success);
                        }
                    } else {
                        println!("Paste queue is empty.");
                    }
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => super::print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }
        QueueAction::Clear => {
            let req = IpcRequest::ClearQueue;
            match IpcClient::send_request(socket_path, &req).await {
                Ok(IpcResponse::Success) => println!("Cleared paste queue."),
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => super::print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }
    }
}
