use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Discovers and sets missing WAYLAND_DISPLAY and DISPLAY environment variables
/// (e.g., when running under systemd user units or minimal environments).
pub fn ensure_display_environment() {
    if std::env::var("WAYLAND_DISPLAY").is_err() {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let p = std::path::Path::new(&runtime_dir);
            for name in &["wayland-0", "wayland-1", "wayland-2"] {
                if p.join(name).exists() {
                    std::env::set_var("WAYLAND_DISPLAY", name);
                    break;
                }
            }
        }
    }

    if std::env::var("DISPLAY").is_err() {
        let x11_dir = std::path::Path::new("/tmp/.X11-unix");
        if x11_dir.exists() {
            for name in &["X0", "X1", "X2"] {
                if x11_dir.join(name).exists() {
                    let disp = format!(":{}", &name[1..]);
                    std::env::set_var("DISPLAY", disp);
                    break;
                }
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
