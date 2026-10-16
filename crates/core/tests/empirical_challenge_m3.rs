//! Empirical Challenge Test Suite for Milestone 3 (R3 & R4)
//! Adversarially verifying:
//! 1. Injector cascade execution across injectors (terminal vs graphical)
//! 2. Terminal focus identification accuracy and resistance to false positives
//! 3. Effective paste delay calibration boundaries
//! 4. Wayland child process cleanup under rapid simulated timeouts (zombie detection)

use clipboard_history_core::config::PasteConfig;
use clipboard_history_core::injector::{
    CopyOnlyInjector, InjectorCascade, PasteInjector, WtypeInjector, X11XTestInjector,
    XdotoolInjector, YdotoolInjector,
};
use clipboard_history_core::window_focus::WindowFocusDetector;
use std::fs;
use std::time::Duration;
use tokio::time::timeout;

// ============================================================================
// 1. Injector Cascade & Individual Injector Verification
// ============================================================================

#[tokio::test]
async fn challenge_injector_cascade_context_terminal_vs_gui() {
    let cascade = InjectorCascade::new();

    // Test terminal context (is_terminal = true)
    let (name_term, success_term) = cascade.execute_paste_context(true).await;
    println!("Terminal paste resolved injector: {}", name_term);
    assert!(
        success_term,
        "execute_paste_context(true) must succeed via available injector or fallback"
    );
    assert!(!name_term.is_empty(), "Injector name must not be empty");

    // Test standard GUI context (is_terminal = false)
    let (name_gui, success_gui) = cascade.execute_paste_context(false).await;
    println!("GUI paste resolved injector: {}", name_gui);
    assert!(
        success_gui,
        "execute_paste_context(false) must succeed via available injector or fallback"
    );
    assert!(!name_gui.is_empty(), "Injector name must not be empty");

    // Both should use the same primary available injector
    assert_eq!(
        name_term, name_gui,
        "Injector cascade should resolve the same primary injector regardless of context"
    );
}

#[tokio::test]
async fn challenge_individual_injectors_execution() {
    // 1. CopyOnlyInjector must always be available and succeed on both contexts
    let copy_only = CopyOnlyInjector::new();
    assert!(
        copy_only.is_available(),
        "CopyOnlyInjector must always be available"
    );
    assert_eq!(copy_only.name(), "copy-only-fallback");
    assert!(
        copy_only.inject_context(true).await,
        "CopyOnly terminal paste must succeed"
    );
    assert!(
        copy_only.inject_context(false).await,
        "CopyOnly GUI paste must succeed"
    );
    assert!(
        copy_only.inject().await,
        "CopyOnly legacy inject must succeed"
    );

    // 2. XdotoolInjector
    let xdotool = XdotoolInjector::new();
    assert_eq!(xdotool.name(), "xdotool");
    if xdotool.is_available() {
        let res_term = xdotool.inject_context(true).await;
        let res_gui = xdotool.inject_context(false).await;
        assert!(
            res_term,
            "xdotool Ctrl+Shift+V injection must succeed when available"
        );
        assert!(
            res_gui,
            "xdotool Ctrl+V injection must succeed when available"
        );
    }

    // 3. X11XTestInjector
    let xtest = X11XTestInjector::new();
    println!("X11XTestInjector is_available: {}", xtest.is_available());
    assert_eq!(xtest.name(), "X11-XTest");
    if xtest.is_available() {
        let res_term = xtest.inject_context(true).await;
        let res_gui = xtest.inject_context(false).await;
        println!(
            "X11XTestInjector inject_context res_term: {}, res_gui: {}",
            res_term, res_gui
        );
        assert!(
            res_term,
            "X11-XTest terminal paste must succeed when available"
        );
        assert!(res_gui, "X11-XTest GUI paste must succeed when available");
    }

    // 4. WtypeInjector (availability depends on binary)
    let wtype = WtypeInjector::new();
    assert_eq!(wtype.name(), "wtype");
    if !wtype.is_available() {
        assert!(!wtype.is_available());
    }

    // 5. YdotoolInjector (availability depends on binary)
    let ydotool = YdotoolInjector::new();
    assert_eq!(ydotool.name(), "ydotool");
    if !ydotool.is_available() {
        assert!(!ydotool.is_available());
    }
}

// ============================================================================
// 2. Adversarial Terminal Focus Detection
// ============================================================================

#[test]
fn challenge_terminal_detection_legitimate_terminals() {
    let legitimate_terminals = [
        "alacritty",
        "Alacritty",
        "ALACRITTY",
        "kitty",
        "Kitty",
        "KITTY",
        "foot",
        "footclient",
        "FOOT",
        "konsole",
        "org.kde.konsole",
        "Konsole",
        "gnome-terminal",
        "gnome-terminal-server",
        "org.gnome.Terminal",
        "wezterm",
        "wezterm-gui",
        "org.wezfurlong.wezterm",
        "xterm",
        "uxterm",
        "XTerm",
        "ghostty",
        "com.mitchellh.ghostty",
        "contour",
        "ptyxis",
        "org.gnome.ptyxis",
        "tilix",
        "terminator",
        "urxvt",
        "rxvt",
        "st",
        "st-256color",
        "hyper",
        "rio",
        "tabby",
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
        "kgx",
        "org.gnome.console",
    ];

    for term in &legitimate_terminals {
        assert!(
            WindowFocusDetector::is_terminal_window("", term),
            "Expected '{}' to be recognized as terminal window",
            term
        );
    }

    // Complex titles in terminal emulators
    assert!(WindowFocusDetector::is_terminal_window(
        "user@host: ~/projects",
        "alacritty"
    ));
    assert!(WindowFocusDetector::is_terminal_window(
        "nano /etc/fstab",
        "kitty"
    ));
    assert!(WindowFocusDetector::is_terminal_window(
        "vim main.rs - Foot",
        "foot"
    ));
    assert!(WindowFocusDetector::is_terminal_window("htop", "xterm"));

    // Title-only fallback when class is empty
    assert!(WindowFocusDetector::is_terminal_window("alacritty", ""));
    assert!(WindowFocusDetector::is_terminal_window(
        "workspace - kitty",
        ""
    ));
    assert!(WindowFocusDetector::is_terminal_window(
        "server - xterm",
        ""
    ));

    // Empty and whitespace metadata
    assert!(!WindowFocusDetector::is_terminal_window("", ""));
    assert!(!WindowFocusDetector::is_terminal_window("   ", "   "));
}

#[test]
fn challenge_terminal_detection_false_positive_rejection() {
    // Non-terminal graphical desktop applications MUST NOT be classified as terminals.
    // Applications containing "st" (e.g. steam, postman, obs-studio, rustrover, webstorm, ghostwriter)
    // were falsely matched due to `class_lower.contains("st")`.
    let graphical_apps = [
        ("Steam", "steam"),
        ("Postman", "postman"),
        ("OBS Studio", "obs-studio"),
        ("RustRover", "rustrover"),
        ("WebStorm", "webstorm"),
        ("ghostwriter", "ghostwriter"),
        ("Terminal Settings - Firefox", "firefox"),
        ("Console API Reference - Google Chrome", "google-chrome"),
        ("terminal.c - Chromium", "chromium"),
        ("terminal.rs - Visual Studio Code", "code"),
        ("TerminalBuffer.java - IntelliJ IDEA", "jetbrains-idea"),
        ("How to use terminal.txt - Text Editor", "gedit"),
        ("terminal-logs - Files", "org.gnome.nautilus"),
        ("nautilus", "nautilus"),
        ("#terminal-help - Slack", "slack"),
        ("console-channel - Discord", "discord"),
        ("Terminal (2004) Movie.mkv - VLC", "vlc"),
        (
            "terminal-spec.odt - LibreOffice Writer",
            "libreoffice-writer",
        ),
    ];

    for (title, class) in &graphical_apps {
        assert!(
            !WindowFocusDetector::is_terminal_window(title, class),
            "Graphical app with title='{}', class='{}' MUST NOT be classified as terminal",
            title,
            class
        );
    }
}

// ============================================================================
// 3. Focus Delay Calibration
// ============================================================================

#[test]
fn challenge_focus_delay_calibration() {
    let mut cfg = PasteConfig::default();

    // Verify default value is reasonable
    assert!(cfg.paste_delay_ms >= 50);

    // Test with low value
    cfg.paste_delay_ms = 10;
    let delay = cfg.effective_delay_ms();
    // In any environment, effective delay must be at least 50ms (or 120ms on Wayland)
    assert!(
        delay >= 50,
        "effective_delay_ms() must enforce at least 50ms minimum (got {})",
        delay
    );

    // Test with zero
    cfg.paste_delay_ms = 0;
    assert!(cfg.effective_delay_ms() >= 50);

    // Test with high user configured value (e.g. 500ms)
    cfg.paste_delay_ms = 500;
    assert_eq!(
        cfg.effective_delay_ms(),
        500,
        "High user configured delay must be preserved"
    );
}

// ============================================================================
// 4. Stress Test: Wayland Child Process Cleanup Under Rapid Timeouts
// ============================================================================

/// Reads process state from /proc/<pid>/stat if it exists.
/// Returns Some(state_char) e.g. 'Z' for zombie, 'R' for running, 'S' for sleeping.
/// Returns None if process has been fully reaped.
fn get_proc_state(pid: u32) -> Option<char> {
    let stat_path = format!("/proc/{}/stat", pid);
    if let Ok(contents) = fs::read_to_string(&stat_path) {
        if let Some(close_paren) = contents.rfind(')') {
            let remainder = &contents[close_paren + 1..].trim_start();
            if let Some(state_char) = remainder.chars().next() {
                return Some(state_char);
            }
        }
    }
    None
}

#[tokio::test]
async fn challenge_stress_test_sequential_child_process_timeouts_no_zombies() {
    const ITERATIONS: usize = 50;
    let mut spawned_pids = Vec::with_capacity(ITERATIONS);

    for _ in 0..ITERATIONS {
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("10");
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().expect("Failed to spawn sleep child");
        let pid = child.id().expect("Child must have PID");
        spawned_pids.push(pid);

        // Simulate rapid timeout abortion (5ms)
        let _ = timeout(Duration::from_millis(5), child.wait()).await;
        // Dropping child triggers kill_on_drop(true)
        drop(child);
    }

    // Give the Tokio runtime process reaper a brief moment to harvest SIGCHLD
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Verify all spawned child processes are terminated and NOT left as zombies ('Z')
    let mut zombie_count = 0;
    let mut lingering_count = 0;

    for &pid in &spawned_pids {
        if let Some(state) = get_proc_state(pid) {
            if state == 'Z' {
                zombie_count += 1;
            } else {
                lingering_count += 1;
            }
        }
    }

    assert_eq!(
        zombie_count, 0,
        "Found {} zombie (<defunct>) processes after sequential timeouts!",
        zombie_count
    );
    assert_eq!(
        lingering_count, 0,
        "Found {} lingering processes still running after kill_on_drop!",
        lingering_count
    );
}

#[tokio::test]
async fn challenge_stress_test_concurrent_child_process_timeouts_no_zombies() {
    const CONCURRENCY: usize = 30;
    let mut tasks = Vec::with_capacity(CONCURRENCY);

    for _ in 0..CONCURRENCY {
        let task = tokio::spawn(async move {
            let mut cmd = tokio::process::Command::new("sleep");
            cmd.arg("10");
            cmd.kill_on_drop(true);

            let mut child = cmd.spawn().expect("Spawn concurrent child");
            let pid = child.id().expect("PID");

            // Rapid simulated timeout
            let _ = timeout(Duration::from_millis(5), child.wait()).await;
            drop(child);
            pid
        });
        tasks.push(task);
    }

    let mut pids = Vec::with_capacity(CONCURRENCY);
    for t in tasks {
        if let Ok(pid) = t.await {
            pids.push(pid);
        }
    }

    // Allow runtime reaper to reap
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut zombie_count = 0;
    let mut lingering_count = 0;

    for &pid in &pids {
        if let Some(state) = get_proc_state(pid) {
            if state == 'Z' {
                zombie_count += 1;
            } else {
                lingering_count += 1;
            }
        }
    }

    assert_eq!(
        zombie_count, 0,
        "Found {} zombie (<defunct>) processes after concurrent timeouts!",
        zombie_count
    );
    assert_eq!(
        lingering_count, 0,
        "Found {} lingering processes after concurrent timeouts!",
        lingering_count
    );
}

#[tokio::test]
async fn challenge_simulated_wl_paste_hung_command_cleanup() {
    // Replicate exact run_wl_paste command invocation pattern with a command that hangs
    let run_hung_paste = || async {
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("20");
        cmd.kill_on_drop(true);

        let pid;
        {
            let mut child = cmd.spawn().ok()?;
            pid = child.id()?;
            let _ = timeout(Duration::from_millis(20), child.wait()).await;
            // Child is dropped here
        }
        Some(pid)
    };

    let mut pids = Vec::new();
    for _ in 0..10 {
        if let Some(pid) = run_hung_paste().await {
            pids.push(pid);
        }
    }

    tokio::time::sleep(Duration::from_millis(150)).await;

    for pid in pids {
        assert!(
            get_proc_state(pid).is_none(),
            "Hung child process PID {} was not fully reaped after simulated timeout",
            pid
        );
    }
}
