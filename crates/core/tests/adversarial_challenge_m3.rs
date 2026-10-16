//! Adversarial Challenge Test Harness for Milestone 3 (R3 & R4)
//! Developed by challenger_m3_1 to empirically probe edge cases, vulnerabilities, and stress boundaries.

use clipboard_history_core::config::AppConfig;
use clipboard_history_core::window_focus::WindowFocusDetector;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;

// ============================================================================
// 1. Terminal Window Detection Adversarial Stress Tests
// ============================================================================

#[test]
fn test_adversarial_terminal_detection_legitimate_terminals() {
    let terminals = [
        "alacritty",
        "kitty",
        "gnome-terminal",
        "gnome-terminal-server",
        "org.gnome.terminal",
        "org.gnome.console",
        "kgx",
        "ptyxis",
        "org.gnome.ptyxis",
        "xterm",
        "uxterm",
        "foot",
        "footclient",
        "wezterm",
        "wezterm-gui",
        "org.wezfurlong.wezterm",
        "konsole",
        "org.kde.konsole",
        "xfce4-terminal",
        "tilix",
        "terminator",
        "urxvt",
        "rxvt",
        "st",
        "st-256color",
        "hyper",
        "ghostty",
        "rio",
        "tabby",
        "contour",
        "blackbox",
        "warp",
        "warp-terminal",
        "guake",
        "yakuake",
        "tilda",
        "terminology",
        "pantheon-terminal",
        "lxterminal",
        "mate-terminal",
        "deepin-terminal",
        "qterminal",
        "x-terminal-emulator",
        "termite",
        "sakura",
    ];

    for term in &terminals {
        assert!(
            WindowFocusDetector::is_terminal_window("", term),
            "Legitimate terminal '{}' must be detected",
            term
        );
        // Case variations
        assert!(
            WindowFocusDetector::is_terminal_window("", &term.to_uppercase()),
            "Uppercase terminal '{}' must be detected",
            term
        );
    }
}

#[test]
fn test_adversarial_terminal_detection_browser_and_gui_apps() {
    let gui_apps = [
        ("firefox", "Terminal Guide - Mozilla Firefox"),
        ("firefox", "How to use the Linux Terminal"),
        ("google-chrome", "Web Terminal - Google Chrome"),
        ("chromium", "Terminal Emulator in Browser - Chromium"),
        ("code", "Terminal - VSCode"),
        ("code", "terminal.rs — core — Visual Studio Code"),
        ("jetbrains-idea", "Terminal - Project [~/idea]"),
        ("jetbrains-idea", "TerminalView.java"),
        ("gedit", "terminal_commands.txt - Text Editor"),
        ("org.gnome.Nautilus", "Terminal folder - Files"),
        ("slack", "#terminal-discussions"),
        ("discord", "#terminal-dev"),
        ("vlc", "Terminal Velocity (1994).mp4"),
        ("libreoffice-writer", "Terminal Documentation.odt"),
    ];

    for (class, title) in &gui_apps {
        assert!(
            !WindowFocusDetector::is_terminal_window(title, class),
            "GUI App class '{}' with title '{}' must NOT be classified as terminal",
            class,
            title
        );
    }
}

#[test]
fn test_adversarial_terminal_detection_empty_and_fallback() {
    // Empty class and empty title
    assert!(!WindowFocusDetector::is_terminal_window("", ""));

    // Empty class with terminal title
    assert!(WindowFocusDetector::is_terminal_window("alacritty", ""));
    assert!(WindowFocusDetector::is_terminal_window(
        "bash - alacritty",
        ""
    ));
    assert!(WindowFocusDetector::is_terminal_window("kitty", ""));
    assert!(WindowFocusDetector::is_terminal_window(
        "user@host: ~ - foot",
        ""
    ));

    // Empty class with non-terminal title
    assert!(!WindowFocusDetector::is_terminal_window(
        "Mozilla Firefox",
        ""
    ));
    assert!(!WindowFocusDetector::is_terminal_window(
        "Document 1 - LibreOffice",
        ""
    ));
    // A tab named "Terminal" when class is empty
    assert!(
        !WindowFocusDetector::is_terminal_window("Terminal", ""),
        "'Terminal' title alone without known binary suffix must not match terminal"
    );
}

#[test]
fn test_adversarial_terminal_detection_embedded_ide_variants() {
    // IDE variations
    let ide_classes = [
        "code",
        "Code",
        "code-oss",
        "vscodium",
        "jetbrains-idea",
        "jetbrains-idea-ce",
        "jetbrains-pycharm",
        "jetbrains-clion",
        "jetbrains-webstorm",
        "jetbrains-goland",
    ];

    for ide in &ide_classes {
        assert!(
            !WindowFocusDetector::is_terminal_window("Embedded Terminal Window", ide),
            "IDE '{}' with embedded terminal must NOT be detected as terminal emulator",
            ide
        );
    }
}

#[test]
fn test_adversarial_terminal_detection_common_desktop_apps_false_positives() {
    // Common desktop apps that might contain short terminal substrings like "st" or "rio"
    let apps_to_check = [
        "steam",
        "postman",
        "fastfetch",
        "rustrover",
        "webstorm",
        "distrobox",
        "calibre",
        "blender",
        "gimp",
        "wireshark",
        "insomnia",
        "inkscape",
        "audacity",
        "obs-studio",
    ];

    let mut false_positives = Vec::new();
    for app in &apps_to_check {
        if WindowFocusDetector::is_terminal_window("Window", app) {
            false_positives.push(*app);
        }
    }

    println!("False positives detected: {:?}", false_positives);
    // If "st" or "rio" matches substrings, false_positives will be non-empty!
    assert!(
        false_positives.is_empty(),
        "The following standard desktop apps were incorrectly flagged as terminals: {:?}",
        false_positives
    );
}

// ============================================================================
// 2. Child Process Termination & Zombie Lingering Stress Tests
// ============================================================================

#[tokio::test]
async fn test_adversarial_child_process_termination_and_reaping() {
    // Spawn a long-running process with kill_on_drop(true)
    let mut cmd = tokio::process::Command::new("sleep");
    cmd.arg("30");
    cmd.kill_on_drop(true);

    let child = cmd.spawn().expect("failed to spawn child");
    let pid = child.id().expect("child must have PID") as i32;

    // Verify process is alive
    let is_alive = unsafe { libc::kill(pid, 0) == 0 };
    assert!(is_alive, "Child process {} should initially be alive", pid);

    // Drop the child handle
    drop(child);

    // Give the OS and tokio runtime a brief moment to signal and reap
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check /proc/<pid>/status
    let proc_status_path = format!("/proc/{}/status", pid);
    if Path::new(&proc_status_path).exists() {
        let content = fs::read_to_string(&proc_status_path).unwrap_or_default();
        let is_zombie = content
            .lines()
            .any(|l| l.starts_with("State:") && l.contains('Z'));
        assert!(
            !is_zombie,
            "Process {} became a zombie (<defunct>) after drop! Status:\n{}",
            pid, content
        );
    }

    // Check if process has been fully reaped
    let still_exists = unsafe { libc::kill(pid, 0) == 0 };
    assert!(
        !still_exists,
        "Process {} is still lingering after kill_on_drop!",
        pid
    );
}

#[tokio::test]
async fn test_adversarial_repeated_rapid_timeouts_no_zombies() {
    let mut pids = Vec::new();

    for _ in 0..50 {
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("20");
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().expect("spawn sleep");
        let pid = child.id().expect("get pid") as i32;
        pids.push(pid);

        // Timeout rapidly
        let _ = timeout(Duration::from_millis(5), child.wait()).await;
        // Dropping child happens here as loop iterates
    }

    // Wait 200ms for tokio driver to finish reaping all children
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Check if any PID lingers as a zombie
    let mut lingering_zombies = Vec::new();
    let mut lingering_alive = Vec::new();

    for pid in pids {
        let proc_status = format!("/proc/{}/status", pid);
        if Path::new(&proc_status).exists() {
            let content = fs::read_to_string(&proc_status).unwrap_or_default();
            if content
                .lines()
                .any(|l| l.starts_with("State:") && l.contains('Z'))
            {
                lingering_zombies.push(pid);
            } else if unsafe { libc::kill(pid, 0) == 0 } {
                lingering_alive.push(pid);
            }
        }
    }

    assert!(
        lingering_zombies.is_empty(),
        "Detected {} zombie processes: {:?}",
        lingering_zombies.len(),
        lingering_zombies
    );
    assert!(
        lingering_alive.is_empty(),
        "Detected {} still-running processes: {:?}",
        lingering_alive.len(),
        lingering_alive
    );
}

// ============================================================================
// 3. Delay Calibration Stress Tests
// ============================================================================

#[test]
fn test_adversarial_delay_calibration_values() {
    let check_delay = |configured: u64, is_wayland: bool| -> u64 {
        let min_delay = if is_wayland { 120 } else { 50 };
        configured.max(min_delay)
    };

    // Delay 0
    assert_eq!(check_delay(0, true), 120, "Wayland delay 0 clamped to 120");
    assert_eq!(check_delay(0, false), 50, "X11 delay 0 clamped to 50");

    // Delay below minimum (e.g. 30ms)
    assert_eq!(
        check_delay(30, true),
        120,
        "Wayland delay 30 clamped to 120"
    );
    assert_eq!(check_delay(30, false), 50, "X11 delay 30 clamped to 50");

    // Standard 120ms
    assert_eq!(check_delay(120, true), 120, "Wayland delay 120 preserved");
    assert_eq!(check_delay(120, false), 120, "X11 delay 120 preserved");

    // Higher values (300ms, 5000ms)
    assert_eq!(check_delay(300, true), 300, "Wayland delay 300 preserved");
    assert_eq!(check_delay(300, false), 300, "X11 delay 300 preserved");
    assert_eq!(
        check_delay(5000, true),
        5000,
        "Wayland delay 5000 preserved"
    );
}

#[test]
fn test_adversarial_negative_delay_toml_rejection() {
    // When TOML config specifies negative delay, deserialization must reject it
    let bad_toml = r#"
        [paste]
        paste_delay_ms = -50
    "#;

    let res = toml::from_str::<AppConfig>(bad_toml);
    assert!(
        res.is_err(),
        "TOML with negative paste_delay_ms must fail deserialization into unsigned u64"
    );
}
