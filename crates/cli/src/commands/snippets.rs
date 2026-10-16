use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use std::path::Path;

#[derive(clap::Subcommand, Debug)]
pub enum SnippetAction {
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

pub async fn handle_snippets(socket_path: &Path, action: Option<SnippetAction>) {
    match action.unwrap_or(SnippetAction::List) {
        SnippetAction::List => {
            let req = IpcRequest::ListSnippets { category: None };
            match IpcClient::send_request(socket_path, &req).await {
                Ok(IpcResponse::Snippets(snippets)) => {
                    if snippets.is_empty() {
                        println!("No snippets saved yet. Add one with: clipboard-history snippets add <label> <content>");
                        return;
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
                Err(_) => super::print_daemon_offline_error(),
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
            match IpcClient::send_request(socket_path, &req).await {
                Ok(IpcResponse::Snippet(s)) => {
                    println!("Created snippet '{}' (ID: {})", s.label, s.id)
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => super::print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }
        SnippetAction::Delete { id } => {
            let req = IpcRequest::DeleteSnippet { id: id.clone() };
            match IpcClient::send_request(socket_path, &req).await {
                Ok(IpcResponse::Success) => println!("Deleted snippet {}", id),
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
                Err(_) => super::print_daemon_offline_error(),
                _ => eprintln!("Unexpected response"),
            }
        }
    }
}
