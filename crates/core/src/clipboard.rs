use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, warn};

const SYSTEMD_ENV_VARS: &[&str] = &[
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "XDG_SESSION_TYPE",
    "XDG_CURRENT_DESKTOP",
    "DBUS_SESSION_BUS_ADDRESS",
    "HYPRLAND_INSTANCE_SIGNATURE",
    "SWAYSOCK",
    "KDE_SESSION_VERSION",
];

fn is_wayland_socket_active(runtime_dir: Option<&Path>, name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.ends_with(".lock") {
        return false;
    }
    let sock = if trimmed.starts_with('/') {
        PathBuf::from(trimmed)
    } else if let Some(dir) = runtime_dir {
        dir.join(trimmed)
    } else {
        return false;
    };
    if !sock.exists() {
        return false;
    }
    #[cfg(unix)]
    {
        std::os::unix::net::UnixStream::connect(&sock).is_ok()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(crate) fn parse_x11_display_number(display: &str) -> Option<&str> {
    let trimmed = display.trim().trim_start_matches(':');
    let num = trimmed.split('.').next()?.trim();
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(num)
}

fn is_x11_socket_active(x11_dir: &Path, display: &str) -> bool {
    let Some(num) = parse_x11_display_number(display) else {
        return false;
    };
    let sock = x11_dir.join(format!("X{}", num));
    if !sock.exists() {
        return false;
    }
    #[cfg(unix)]
    {
        std::os::unix::net::UnixStream::connect(&sock).is_ok()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn resolve_runtime_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        let fallback = PathBuf::from(format!("/run/user/{}", uid));
        if fallback.is_dir() {
            std::env::set_var("XDG_RUNTIME_DIR", &fallback);
            return Some(fallback);
        }
    }

    None
}

fn read_systemd_user_environment() -> HashMap<String, String> {
    let mut env_map = HashMap::new();
    if which::which("systemctl").is_err() {
        return env_map;
    }

    let mut child = match std::process::Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to invoke systemctl --user show-environment: {}", e);
            return env_map;
        }
    };

    let mut stdout_pipe = child.stdout.take();
    let deadline = std::time::Instant::now() + Duration::from_millis(800);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    debug!("systemctl --user show-environment exited with {}", status);
                    return env_map;
                }
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    debug!("systemctl --user show-environment timed out");
                    return env_map;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                debug!("Error polling systemctl --user show-environment: {}", e);
                return env_map;
            }
        }
    }

    let mut buf = Vec::new();
    if let Some(ref mut out) = stdout_pipe {
        let _ = std::io::Read::read_to_end(out, &mut buf);
    }

    let stdout = String::from_utf8_lossy(&buf);
    for line in stdout.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if SYSTEMD_ENV_VARS.contains(&k) && !v.is_empty() {
                env_map.insert(k.to_string(), v.to_string());
            }
        }
    }

    env_map
}

fn find_active_wayland_socket(runtime_dir: &Path) -> Option<String> {
    let entries = std::fs::read_dir(runtime_dir).ok()?;
    let mut candidates: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if name.starts_with("wayland-") && !name.ends_with(".lock") {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    candidates.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

    candidates
        .into_iter()
        .find(|name| is_wayland_socket_active(Some(runtime_dir), name))
}

fn find_active_x11_display(x11_dir: &Path) -> Option<String> {
    let entries = std::fs::read_dir(x11_dir).ok()?;
    let mut displays: Vec<u32> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let num_str = name.strip_prefix('X')?;
            if num_str.is_empty() || !num_str.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            num_str.parse::<u32>().ok()
        })
        .collect();

    displays.sort_unstable();
    displays.into_iter().find_map(|num| {
        let disp = format!(":{}", num);
        if is_x11_socket_active(x11_dir, &disp) {
            Some(disp)
        } else {
            None
        }
    })
}

fn ensure_display_environment_inner(force: bool) {
    static PROBE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    static LAST_PROBE_MS: AtomicU64 = AtomicU64::new(0);

    let _guard = PROBE_MUTEX
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let runtime_dir = resolve_runtime_dir();
    let x11_dir = Path::new("/tmp/.X11-unix");

    let mut wayland_valid = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .is_some_and(|disp| is_wayland_socket_active(runtime_dir.as_deref(), &disp));
    let mut x11_valid = std::env::var("DISPLAY")
        .ok()
        .is_some_and(|disp| is_x11_socket_active(x11_dir, &disp));

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    if !force && wayland_valid && x11_valid {
        let last = LAST_PROBE_MS.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last) < 15_000 {
            return;
        }
    }
    LAST_PROBE_MS.store(now_ms, Ordering::Relaxed);

    if force || !wayland_valid || !x11_valid {
        let systemd_env = read_systemd_user_environment();
        if !systemd_env.is_empty() {
            if let Some(wd) = systemd_env.get("WAYLAND_DISPLAY") {
                let candidate_ok =
                    runtime_dir.is_none() || is_wayland_socket_active(runtime_dir.as_deref(), wd);
                if (!wayland_valid || force) && candidate_ok {
                    std::env::set_var("WAYLAND_DISPLAY", wd);
                    wayland_valid = is_wayland_socket_active(runtime_dir.as_deref(), wd);
                }
            }

            if let Some(disp) = systemd_env.get("DISPLAY") {
                let candidate_ok = !x11_dir.exists() || is_x11_socket_active(x11_dir, disp);
                if (!x11_valid || force) && candidate_ok {
                    std::env::set_var("DISPLAY", disp);
                    x11_valid = is_x11_socket_active(x11_dir, disp);
                }
            }

            if let Some(sway) = systemd_env.get("SWAYSOCK") {
                let cur_valid = std::env::var("SWAYSOCK")
                    .ok()
                    .is_some_and(|v| !v.trim().is_empty() && Path::new(&v).exists());
                if (!cur_valid || force) && Path::new(sway).exists() {
                    std::env::set_var("SWAYSOCK", sway);
                }
            }

            for key in [
                "XDG_SESSION_TYPE",
                "XDG_CURRENT_DESKTOP",
                "DBUS_SESSION_BUS_ADDRESS",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "KDE_SESSION_VERSION",
            ] {
                if let Some(val) = systemd_env.get(key) {
                    let cur_missing = std::env::var(key)
                        .map(|v| v.trim().is_empty())
                        .unwrap_or(true);
                    if cur_missing || force {
                        std::env::set_var(key, val);
                    }
                }
            }
        }
    }

    let found_wayland = if !wayland_valid {
        runtime_dir.as_deref().and_then(find_active_wayland_socket)
    } else {
        None
    };
    if let Some(sock_name) = found_wayland {
        std::env::set_var("WAYLAND_DISPLAY", &sock_name);
        let session_missing = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.trim().is_empty())
            .unwrap_or(true);
        if session_missing {
            std::env::set_var("XDG_SESSION_TYPE", "wayland");
        }
    }

    let found_x11 = if !x11_valid && x11_dir.exists() {
        find_active_x11_display(x11_dir)
    } else {
        None
    };
    if let Some(disp) = found_x11 {
        std::env::set_var("DISPLAY", disp);
    }
}

/// Discovers and sets missing or stale WAYLAND_DISPLAY and DISPLAY environment variables
/// (e.g., when running under systemd user units or minimal environments).
pub fn ensure_display_environment() {
    ensure_display_environment_inner(false);
}

/// Forces an immediate re-probe of live display environment variables and sockets,
/// bypassing the 15-second throttle.
pub fn ensure_display_environment_force() {
    ensure_display_environment_inner(true);
}

async fn run_clipboard_setter(program: &str, args: &[&str], data: &[u8]) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to spawn `{}`: {}", program, e);
            return false;
        }
    };

    let result = tokio::time::timeout(Duration::from_millis(2500), async {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(data).await {
                return Err(format!("stdin write failed: {}", e));
            }
            drop(stdin);
        }
        match child.wait().await {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!("exited with status {}", status)),
            Err(e) => Err(format!("wait failed: {}", e)),
        }
    })
    .await;

    match result {
        Ok(Ok(())) => true,
        Ok(Err(reason)) => {
            debug!("`{}` failed: {}", program, reason);
            false
        }
        Err(_) => {
            warn!("`{}` timed out after 2.5s", program);
            false
        }
    }
}

/// Copies text to the OS display server clipboard (Wayland wl-copy or X11 xclip/xsel).
pub async fn copy_to_system_clipboard(text: &str) {
    ensure_display_environment();

    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .eq_ignore_ascii_case("wayland");

    // 1. Try Wayland wl-copy if Wayland session is active
    if has_wayland
        && which::which("wl-copy").is_ok()
        && run_clipboard_setter("wl-copy", &[], text.as_bytes()).await
    {
        return;
    }

    // 2. Fallback to X11 xclip
    if which::which("xclip").is_ok()
        && run_clipboard_setter("xclip", &["-selection", "clipboard"], text.as_bytes()).await
    {
        return;
    }

    // 3. Fallback to X11 xsel
    if which::which("xsel").is_ok() {
        let _ = run_clipboard_setter("xsel", &["--clipboard", "--input"], text.as_bytes()).await;
    }
}

/// Copies image bytes to the OS display server clipboard (Wayland wl-copy or X11 xclip).
pub async fn copy_image_to_system_clipboard(image_bytes: &[u8], mime_type: Option<&str>) {
    ensure_display_environment();
    let mime = mime_type.unwrap_or("image/png");

    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .eq_ignore_ascii_case("wayland");

    // 1. Try Wayland wl-copy if Wayland session is active
    if has_wayland
        && which::which("wl-copy").is_ok()
        && run_clipboard_setter("wl-copy", &["--type", mime], image_bytes).await
    {
        return;
    }

    // 2. Fallback to X11 xclip
    if which::which("xclip").is_ok() {
        let _ = run_clipboard_setter(
            "xclip",
            &["-selection", "clipboard", "-t", mime],
            image_bytes,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_display_environment_runs_safely() {
        ensure_display_environment();
        ensure_display_environment_force();
    }

    #[test]
    fn test_parse_x11_display_number() {
        assert_eq!(parse_x11_display_number(":0.0"), Some("0"));
        assert_eq!(parse_x11_display_number(":0"), Some("0"));
        assert_eq!(parse_x11_display_number(":1.2"), Some("1"));
        assert_eq!(parse_x11_display_number(":12.0"), Some("12"));
        assert_eq!(parse_x11_display_number(" :3.1 "), Some("3"));
        assert_eq!(parse_x11_display_number(""), None);
        assert_eq!(parse_x11_display_number(":"), None);
        assert_eq!(parse_x11_display_number(":.0"), None);
        assert_eq!(parse_x11_display_number("localhost:10.0"), None);
    }

    #[cfg(unix)]
    #[test]
    fn test_find_active_wayland_socket_ignores_lock_files() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("wayland-0.lock"), b"").expect("write lock");
        std::fs::write(dir.path().join("wayland-0"), b"").expect("write dead socket file");
        let _listener = UnixListener::bind(dir.path().join("wayland-1")).expect("bind unix socket");

        assert!(!is_wayland_socket_active(
            Some(dir.path()),
            "wayland-0.lock"
        ));
        assert!(!is_wayland_socket_active(Some(dir.path()), "wayland-0"));
        assert!(!is_wayland_socket_active(Some(dir.path()), ""));
        assert!(is_wayland_socket_active(Some(dir.path()), "wayland-1"));
        assert_eq!(
            find_active_wayland_socket(dir.path()),
            Some("wayland-1".to_string())
        );
    }
}
