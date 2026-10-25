use tokio::io::AsyncWriteExt;
use tokio::process::Command;

fn is_wayland_socket_active(runtime_dir: &std::path::Path, name: &str) -> bool {
    let sock = if name.starts_with('/') {
        std::path::PathBuf::from(name)
    } else {
        runtime_dir.join(name)
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

/// Discovers and sets missing or stale WAYLAND_DISPLAY and DISPLAY environment variables
/// (e.g., when running under systemd user units or minimal environments).
pub fn ensure_display_environment() {
    static LAST_PROBE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    let has_display = std::env::var("DISPLAY").is_ok();

    // Fast path: if both environment variables already exist, skip probing unless 15 seconds have passed
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    if has_wayland && has_display {
        let last = LAST_PROBE_MS.load(std::sync::atomic::Ordering::Relaxed);
        if now_ms.saturating_sub(last) < 15_000 {
            return;
        }
    }
    LAST_PROBE_MS.store(now_ms, std::sync::atomic::Ordering::Relaxed);

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let p = std::path::Path::new(&runtime_dir);
        let current_valid = std::env::var("WAYLAND_DISPLAY")
            .ok()
            .map(|disp| is_wayland_socket_active(p, &disp))
            .unwrap_or(false);

        if !current_valid {
            for i in 0..=9 {
                let name = format!("wayland-{}", i);
                if is_wayland_socket_active(p, &name) {
                    std::env::set_var("WAYLAND_DISPLAY", &name);
                    std::env::set_var("XDG_SESSION_TYPE", "wayland");
                    break;
                }
            }
        }
    }

    let x11_dir = std::path::Path::new("/tmp/.X11-unix");
    let x11_valid = std::env::var("DISPLAY")
        .ok()
        .map(|disp| {
            let num = disp.trim_start_matches(':');
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
        })
        .unwrap_or(false);

    if !x11_valid && x11_dir.exists() {
        for i in 0..=9 {
            let name = format!("X{}", i);
            let sock = x11_dir.join(&name);
            #[cfg(unix)]
            let active = sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok();
            #[cfg(not(unix))]
            let active = sock.exists();

            if active {
                std::env::set_var("DISPLAY", format!(":{}", i));
                break;
            }
        }
    }
}

/// Copies text to the OS display server clipboard (Wayland wl-copy or X11 xclip/xsel).
pub async fn copy_to_system_clipboard(text: &str) {
    ensure_display_environment();

    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland";

    // 1. Try Wayland wl-copy if Wayland session is active
    if has_wayland && which::which("wl-copy").is_ok() {
        if let Ok(mut child) = Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes()).await;
                drop(stdin);
            }
            if let Ok(status) = child.wait().await {
                if status.success() {
                    return;
                }
            }
        }
    }

    // 2. Fallback to X11 xclip
    if which::which("xclip").is_ok() {
        if let Ok(mut child) = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes()).await;
                drop(stdin);
            }
            if let Ok(status) = child.wait().await {
                if status.success() {
                    return;
                }
            }
        }
    }

    // 3. Fallback to X11 xsel
    if which::which("xsel").is_ok() {
        if let Ok(mut child) = Command::new("xsel")
            .args(["--clipboard", "--input"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes()).await;
                drop(stdin);
            }
            let _ = child.wait().await;
        }
    }
}

/// Copies image bytes to the OS display server clipboard (Wayland wl-copy or X11 xclip).
pub async fn copy_image_to_system_clipboard(image_bytes: &[u8], mime_type: Option<&str>) {
    ensure_display_environment();
    let mime = mime_type.unwrap_or("image/png");

    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland";

    // 1. Try Wayland wl-copy if Wayland session is active
    if has_wayland && which::which("wl-copy").is_ok() {
        if let Ok(mut child) = Command::new("wl-copy")
            .args(["--type", mime])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(image_bytes).await;
                drop(stdin);
            }
            if let Ok(status) = child.wait().await {
                if status.success() {
                    return;
                }
            }
        }
    }

    // 2. Fallback to X11 xclip
    if which::which("xclip").is_ok() {
        if let Ok(mut child) = Command::new("xclip")
            .args(["-selection", "clipboard", "-t", mime])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(image_bytes).await;
                drop(stdin);
            }
            let _ = child.wait().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_display_environment_runs_safely() {
        ensure_display_environment();
    }
}
