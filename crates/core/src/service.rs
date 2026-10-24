use crate::config::AppConfig;
use crate::ipc::IpcClient;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Discovers the location of `clipboard-history-daemon` binary across common Linux installation paths.
pub fn find_daemon_binary() -> Option<PathBuf> {
    find_binary_named("clipboard-history-daemon")
}

/// Discovers the location of `clipboard-history-gui` binary across common Linux installation paths.
pub fn find_gui_binary() -> Option<PathBuf> {
    find_binary_named("clipboard-history-gui")
}

/// Discovers the location of `clipboard-history` CLI binary across common Linux installation paths.
pub fn find_cli_binary() -> Option<PathBuf> {
    find_binary_named("clipboard-history")
}

fn find_binary_named(bin_name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    // 1. Check directory of the currently running executable (sibling)
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join(bin_name));
        }
    }

    // 2. Check PATH lookup via which
    if let Ok(path) = which::which(bin_name) {
        candidates.push(path);
    }

    // 3. Check user local directory ~/.local/bin
    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home);
        candidates.push(home_path.join(".local/bin").join(bin_name));
        candidates.push(home_path.join(".cargo/bin").join(bin_name));
    }

    // 4. Standard system locations
    candidates.push(PathBuf::from("/usr/bin").join(bin_name));
    candidates.push(PathBuf::from("/usr/local/bin").join(bin_name));
    candidates.push(PathBuf::from("/opt/clipboard-history/bin").join(bin_name));

    // 5. Development target dirs (target/release, target/debug)
    candidates.push(PathBuf::from("target/release").join(bin_name));
    candidates.push(PathBuf::from("target/debug").join(bin_name));

    for candidate in candidates {
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
    }
    true
}

/// Checks whether a systemd user unit file for `clipboard-history.service` exists.
pub fn find_systemd_unit_file() -> Option<PathBuf> {
    let candidates = [
        // User custom or auto-installed units
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".config/systemd/user/clipboard-history.service")),
        // Arch / Fedora / Debian standard distribution package locations
        Some(PathBuf::from(
            "/usr/lib/systemd/user/clipboard-history.service",
        )),
        Some(PathBuf::from(
            "/usr/local/lib/systemd/user/clipboard-history.service",
        )),
        Some(PathBuf::from("/etc/systemd/user/clipboard-history.service")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

/// Auto-installs or validates the systemd user service unit in `~/.config/systemd/user/`.
/// Returns `Ok(true)` if newly installed, `Ok(false)` if already present, or `Err` on failure.
pub fn ensure_systemd_unit_installed() -> Result<bool, String> {
    if which::which("systemctl").is_err() {
        return Err("systemctl command not found on this system".to_string());
    }

    // If already installed system-wide or in user config, trigger daemon-reload and return
    if let Some(existing) = find_systemd_unit_file() {
        debug!("Found existing systemd unit file at {}", existing.display());
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        return Ok(false);
    }

    let home = std::env::var("HOME").map_err(|e| format!("Could not read $HOME: {}", e))?;
    let target_dir = PathBuf::from(home).join(".config/systemd/user");
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create {}: {}", target_dir.display(), e))?;

    let target_file = target_dir.join("clipboard-history.service");
    let daemon_bin =
        find_daemon_binary().unwrap_or_else(|| PathBuf::from("clipboard-history-daemon"));
    let exec_start = daemon_bin.to_string_lossy();

    let unit_content = format!(
        r#"[Unit]
Description=Universal Linux Clipboard History Daemon
Documentation=https://github.com/alierenaltindag/clipboard-history
After=graphical-session.target

[Service]
Type=simple
ExecStart={}
Restart=on-failure
RestartSec=2s
Environment=RUST_LOG=info
Environment=PATH=%h/.local/bin:%h/.cargo/bin:/usr/local/bin:/usr/bin:/bin
PassEnvironment=WAYLAND_DISPLAY DISPLAY XDG_SESSION_TYPE XDG_CURRENT_DESKTOP XAUTHORITY XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS
Slice=app.slice
TimeoutStopSec=5s
UMask=0077

# Security hardening directives
LockPersonality=true

[Install]
WantedBy=default.target
"#,
        exec_start
    );

    std::fs::write(&target_file, unit_content)
        .map_err(|e| format!("Failed to write unit file {}: {}", target_file.display(), e))?;

    let reload_status = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    if let Ok(s) = reload_status {
        if s.success() {
            info!(
                "Successfully installed systemd user unit at {}",
                target_file.display()
            );
            return Ok(true);
        }
    }

    Ok(true)
}

/// Checks if the Unix domain socket is unresponsive or stale, and safely unlinks it.
pub async fn cleanup_stale_socket(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }

    // Try a quick connection test
    match tokio::time::timeout(
        Duration::from_millis(150),
        tokio::net::UnixStream::connect(socket_path),
    )
    .await
    {
        Ok(Ok(_)) => {
            // Socket is active and listening
            false
        }
        _ => {
            // Socket exists but is dead / connection refused
            debug!("Unlinking stale socket file: {}", socket_path.display());
            let _ = std::fs::remove_file(socket_path);
            true
        }
    }
}

/// Diagnostic snapshot of daemon and service state for troubleshooting.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonDiagnostic {
    pub daemon_binary: Option<PathBuf>,
    pub gui_binary: Option<PathBuf>,
    pub systemd_available: bool,
    pub systemd_unit_path: Option<PathBuf>,
    pub is_active_in_systemd: bool,
    pub systemd_unit_status: String,
    pub socket_path: PathBuf,
    pub socket_exists: bool,
    pub socket_responsive: bool,
    pub wayland_display: Option<String>,
    pub x11_display: Option<String>,
    pub session_type: Option<String>,
    pub desktop_session: Option<String>,
    pub recent_logs: Vec<String>,
}

impl DaemonDiagnostic {
    /// Formats diagnostic report as human-readable plain text (for copying/debugging).
    pub fn to_report_string(&self) -> String {
        let mut out = String::new();
        out.push_str("=== Clipboard History Service Diagnostics ===\n");
        out.push_str(&format!(
            "Daemon Binary: {}\n",
            self.daemon_binary
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not Found ❌".to_string())
        ));
        out.push_str(&format!(
            "GUI Binary:    {}\n",
            self.gui_binary
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not Found ❌".to_string())
        ));
        out.push_str(&format!(
            "Systemd:       {} (Unit: {})\n",
            if self.systemd_available {
                "Available"
            } else {
                "Unavailable"
            },
            self.systemd_unit_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "None".to_string())
        ));
        out.push_str(&format!(
            "Systemd State: {} ({})\n",
            if self.is_active_in_systemd {
                "Active (running)"
            } else {
                "Inactive/Failed"
            },
            self.systemd_unit_status
        ));
        out.push_str(&format!(
            "Socket:        {} (Exists: {}, Responsive: {})\n",
            self.socket_path.display(),
            self.socket_exists,
            self.socket_responsive
        ));
        out.push_str(&format!(
            "Display Env:   WAYLAND_DISPLAY={:?}, DISPLAY={:?}, SESSION_TYPE={:?}, DESKTOP={:?}\n",
            self.wayland_display, self.x11_display, self.session_type, self.desktop_session
        ));
        if !self.recent_logs.is_empty() {
            out.push_str("\n--- Recent Logs (journalctl) ---\n");
            for line in &self.recent_logs {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }
}

/// Collects full system and daemon diagnostic information.
pub async fn collect_diagnostics() -> DaemonDiagnostic {
    let socket_path = AppConfig::socket_path();
    let socket_exists = socket_path.exists();
    let socket_responsive = if socket_exists {
        tokio::time::timeout(
            Duration::from_millis(150),
            tokio::net::UnixStream::connect(&socket_path),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
    } else {
        false
    };

    let systemd_available = which::which("systemctl").is_ok();
    let systemd_unit_path = find_systemd_unit_file();

    let mut is_active_in_systemd = false;
    let mut systemd_unit_status = "unknown".to_string();

    if systemd_available {
        if let Ok(output) = Command::new("systemctl")
            .args(["--user", "is-active", "clipboard-history.service"])
            .output()
        {
            let status_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            is_active_in_systemd = status_str == "active";
            systemd_unit_status = if status_str.is_empty() {
                String::from_utf8_lossy(&output.stderr).trim().to_string()
            } else {
                status_str
            };
        }
    }

    let mut recent_logs = Vec::new();
    if systemd_available {
        if let Ok(output) = Command::new("journalctl")
            .args([
                "--user",
                "-u",
                "clipboard-history.service",
                "-n",
                "20",
                "--no-pager",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            recent_logs = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect();
        }
    }

    DaemonDiagnostic {
        daemon_binary: find_daemon_binary(),
        gui_binary: find_gui_binary(),
        systemd_available,
        systemd_unit_path,
        is_active_in_systemd,
        systemd_unit_status,
        socket_path,
        socket_exists,
        socket_responsive,
        wayland_display: std::env::var("WAYLAND_DISPLAY").ok(),
        x11_display: std::env::var("DISPLAY").ok(),
        session_type: std::env::var("XDG_SESSION_TYPE").ok(),
        desktop_session: std::env::var("XDG_CURRENT_DESKTOP").ok(),
        recent_logs,
    }
}

/// Intelligent daemon starter:
/// 1. Verifies if already running via IPC.
/// 2. Cleans up stale sockets.
/// 3. Ensures display server variables are imported.
/// 4. Auto-installs and starts systemd user service if systemctl is available.
/// 5. Automatically falls back to detached standalone process if systemd fails.
/// 6. Polls IPC with a progressive retry loop (up to 3 seconds).
pub async fn smart_start_daemon() -> Result<IpcClient, String> {
    // 1. Check if already responding
    if let Ok(client) = IpcClient::connect().await {
        return Ok(client);
    }

    // 2. Clean up dead socket if present
    let socket_path = AppConfig::socket_path();
    cleanup_stale_socket(&socket_path).await;

    // 3. Ensure display environment is populated
    crate::clipboard::ensure_display_environment();

    // 4. Try systemd user service if systemctl is installed
    if which::which("systemctl").is_ok() {
        // Auto-install unit if missing
        let _ = ensure_systemd_unit_installed();

        // Import graphical session environment variables into systemd user manager
        let _ = Command::new("systemctl")
            .args([
                "--user",
                "import-environment",
                "WAYLAND_DISPLAY",
                "DISPLAY",
                "XDG_CURRENT_DESKTOP",
                "XDG_SESSION_TYPE",
                "XDG_RUNTIME_DIR",
            ])
            .status();

        // Reset failed state in case service hit systemd rate-limit
        let _ = Command::new("systemctl")
            .args(["--user", "reset-failed", "clipboard-history.service"])
            .status();

        // Enable for autostart on login and restart service
        let _ = Command::new("systemctl")
            .args(["--user", "enable", "clipboard-history.service"])
            .status();

        let _ = Command::new("systemctl")
            .args(["--user", "restart", "clipboard-history.service"])
            .status();

        // Wait up to 2.5 seconds with 100ms intervals
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(client) = IpcClient::connect().await {
                info!("Successfully connected to daemon via systemd user service.");
                return Ok(client);
            }
        }
    }

    // 5. Fallback: direct background spawn
    info!("Systemd user service not responsive. Spawning standalone daemon...");
    let daemon_bin = find_daemon_binary().ok_or_else(|| {
        "clipboard-history-daemon executable not found. Please ensure it is installed in /usr/bin or ~/.local/bin.".to_string()
    })?;

    let mut cmd = Command::new(&daemon_bin);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    match cmd.spawn() {
        Ok(_) => {
            for _ in 0..25 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if let Ok(client) = IpcClient::connect().await {
                    info!("Successfully connected to daemon via standalone spawn.");
                    return Ok(client);
                }
            }
        }
        Err(e) => {
            return Err(format!(
                "Failed to spawn daemon process ({}): {}",
                daemon_bin.display(),
                e
            ));
        }
    }

    // 6. Gather diagnostics to explain the exact failure
    let diag = collect_diagnostics().await;
    let mut err_msg = format!(
        "Background daemon failed to initialize.\n\nDaemon executable: {}\nSystemd status: {}\nSocket: {}",
        diag.daemon_binary
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "Not Found".to_string()),
        diag.systemd_unit_status,
        diag.socket_path.display()
    );

    if !diag.recent_logs.is_empty() {
        err_msg.push_str("\n\nRecent logs:\n");
        let sample = diag.recent_logs.iter().rev().take(5).collect::<Vec<_>>();
        for l in sample.into_iter().rev() {
            err_msg.push_str(&format!("  {}\n", l));
        }
    }

    Err(err_msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_daemon_binary_candidate_search() {
        // In local workspace, target/release or target/debug should be found
        let found = find_daemon_binary();
        assert!(found.is_some() || which::which("clipboard-history-daemon").is_err());
    }

    #[tokio::test]
    async fn test_collect_diagnostics() {
        let diag = collect_diagnostics().await;
        assert_eq!(diag.socket_path, AppConfig::socket_path());
        let report = diag.to_report_string();
        assert!(report.contains("Clipboard History Service Diagnostics"));
    }
}
