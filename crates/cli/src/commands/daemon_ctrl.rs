use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use std::path::Path;
use std::process::Command;

pub async fn handle_toggle(socket_path: &Path) {
    let toggle_req = IpcRequest::ToggleWindow;
    match IpcClient::send_request(socket_path, &toggle_req).await {
        Ok(IpcResponse::Success) => {
            println!("Triggered clipboard history window toggle.");
        }
        Ok(IpcResponse::Error(e)) => {
            eprintln!("Daemon returned error toggling window: {}", e);
            let _ = Command::new("clipboard-history-gui")
                .arg("--toggle")
                .spawn();
        }
        _ => {
            let _ = Command::new("clipboard-history-gui")
                .arg("--toggle")
                .spawn();
            println!("Triggered clipboard history window toggle (standalone).");
        }
    }
}

pub async fn handle_status(socket_path: &Path) {
    let req = IpcRequest::GetStatus;
    match IpcClient::send_request(socket_path, &req).await {
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
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_pause(socket_path: &Path) {
    let req = IpcRequest::SetPause { paused: true };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Success) => println!("Clipboard tracking PAUSED."),
        Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}

pub async fn handle_resume(socket_path: &Path) {
    let req = IpcRequest::SetPause { paused: false };
    match IpcClient::send_request(socket_path, &req).await {
        Ok(IpcResponse::Success) => println!("Clipboard tracking RESUMED."),
        Ok(IpcResponse::Error(e)) => eprintln!("Error: {}", e),
        Err(_) => super::print_daemon_offline_error(),
        _ => eprintln!("Unexpected response"),
    }
}
