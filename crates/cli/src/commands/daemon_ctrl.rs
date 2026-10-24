use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use std::path::Path;
use std::process::Command;

fn get_gui_executable() -> std::path::PathBuf {
    clipboard_history_core::service::find_gui_binary()
        .unwrap_or_else(|| std::path::PathBuf::from("clipboard-history-gui"))
}

pub async fn handle_toggle(_socket_path: &Path) {
    let gui_bin = get_gui_executable();
    let mut cmd = Command::new(&gui_bin);
    cmd.arg("--toggle")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    match cmd.spawn() {
        Ok(_) => {
            println!("Triggered clipboard history window toggle.");
        }
        Err(e) => {
            eprintln!("Failed to spawn {}: {}", gui_bin.display(), e);
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
        Err(_) => {
            super::print_daemon_offline_error();
            let diag = clipboard_history_core::service::collect_diagnostics().await;
            eprintln!(
                "\nService info: systemd_active={}, unit_status={}",
                diag.is_active_in_systemd, diag.systemd_unit_status
            );
        }
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

#[derive(clap::Subcommand, Debug, Clone)]
pub enum DaemonAction {
    #[command(about = "Start background daemon service")]
    Start,
    #[command(about = "Stop background daemon service")]
    Stop,
    #[command(about = "Restart background daemon service")]
    Restart,
    #[command(about = "Display daemon service status")]
    Status,
}

pub async fn handle_daemon(action: Option<DaemonAction>, socket_path: &Path) {
    match action {
        Some(DaemonAction::Start) | None => start_daemon(socket_path).await,
        Some(DaemonAction::Stop) => stop_daemon(socket_path).await,
        Some(DaemonAction::Restart) => restart_daemon(socket_path).await,
        Some(DaemonAction::Status) => handle_status(socket_path).await,
    }
}

async fn start_daemon(socket_path: &Path) {
    // Check if already running
    if let Ok(IpcResponse::Status(_)) =
        IpcClient::send_request(socket_path, &IpcRequest::GetStatus).await
    {
        println!("✓ Clipboard History Daemon is already running and active.");
        return;
    }

    println!("Starting clipboard-history-daemon...");
    match clipboard_history_core::service::smart_start_daemon().await {
        Ok(_) => {
            println!("✓ Clipboard History Daemon successfully started.");
        }
        Err(err) => {
            eprintln!("❌ Failed to start clipboard-history-daemon.\n");
            eprintln!("{}", err);
            eprintln!("\nDiagnostics:");
            eprintln!(
                "  1. Check systemd logs: journalctl --user -u clipboard-history.service -e -n 25"
            );
            eprintln!(
                "  2. Or run manually in foreground to inspect errors: clipboard-history-daemon"
            );
        }
    }
}

async fn stop_daemon(socket_path: &Path) {
    println!("Stopping clipboard-history-daemon...");
    if which::which("systemctl").is_ok() {
        let _ = Command::new("systemctl")
            .args(["--user", "stop", "clipboard-history.service"])
            .status();
    }

    let _ = Command::new("pkill")
        .args(["-f", "clipboard-history-daemon"])
        .status();

    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }

    println!("✓ Clipboard History Daemon stopped.");
}

async fn restart_daemon(socket_path: &Path) {
    stop_daemon(socket_path).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    start_daemon(socket_path).await;
}
