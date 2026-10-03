use crate::clipboard::copy_to_system_clipboard;
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use clipboard_history_core::transforms::{ConcatDelimiter, DiffTag, TextTransforms, UrlCleaner};
use std::path::Path;

pub async fn handle_ocr(socket_path: &Path, id: String) {
    // First try resolving entry by ID to get its blob_hash
    let blob_hash = match IpcClient::send_request(
        socket_path,
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
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::OcrResult { text }) => {
            println!("OCR Extracted Text:\n-------------------");
            println!("{}", text);
        }
        Ok(IpcResponse::Error(e)) => eprintln!("OCR Error: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_join(
    socket_path: &Path,
    delimiter: Option<String>,
    numbered: bool,
    bullets: bool,
    ids: Vec<String>,
) {
    if ids.is_empty() {
        eprintln!("Error: At least one entry ID must be provided to join.");
        return;
    }

    let mut texts = Vec::new();
    for id in &ids {
        let req = IpcRequest::GetEntry { id: id.clone() };
        match IpcClient::send_request(socket_path, &req).await {
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
        return;
    }

    let delim = if numbered {
        ConcatDelimiter::NumberedList
    } else if bullets {
        ConcatDelimiter::BulletList
    } else if let Some(d) = delimiter {
        let unescaped = d.replace("\\n", "\n").replace("\\t", "\t");
        ConcatDelimiter::Custom(unescaped)
    } else {
        ConcatDelimiter::Newline
    };

    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    let joined = TextTransforms::concatenate(&text_refs, &delim);

    // Copy to daemon clipboard history
    let add_req = IpcRequest::AddManualEntry {
        text: joined.clone(),
    };
    let _ = IpcClient::send_request(socket_path, &add_req).await;

    // Set display server clipboard (Wayland / X11) - MED-05
    copy_to_system_clipboard(&joined).await;

    println!(
        "Concatenated {} items ({} bytes) into clipboard:",
        texts.len(),
        joined.len()
    );
    println!("{:-<50}", "");
    println!("{}", joined);
}

pub async fn handle_diff(socket_path: &Path, id_a: String, id_b: String, no_color: bool) {
    let req = IpcRequest::ComputeDiff {
        id_a: id_a.clone(),
        id_b: id_b.clone(),
    };
    match IpcClient::send_request(socket_path, &req).await {
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
                        DiffTag::Insert => {
                            println!("\x1b[32m+ {}\x1b[0m", line.text);
                        }
                        DiffTag::Delete => {
                            println!("\x1b[31m- {}\x1b[0m", line.text);
                        }
                        DiffTag::Equal => {
                            println!("  {}", line.text);
                        }
                    }
                }
            }
        }
        Ok(IpcResponse::Error(e)) => eprintln!("Diff Error: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_clean_url(socket_path: &Path, target: String) {
    let is_url = target.starts_with("http://") || target.starts_with("https://");
    let (original_text, entry_id) = if is_url {
        (target.clone(), None)
    } else {
        // Try fetching entry by ID from daemon
        let req = IpcRequest::GetEntry { id: target.clone() };
        match IpcClient::send_request(socket_path, &req).await {
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

    let cleaned = UrlCleaner::clean_text_urls(&original_text);

    if cleaned == original_text {
        println!("No tracking parameters detected in target:");
        println!("{}", original_text);
    } else {
        // Add cleaned entry to clipboard history
        let add_req = IpcRequest::AddManualEntry {
            text: cleaned.clone(),
        };
        let _ = IpcClient::send_request(socket_path, &add_req).await;

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
