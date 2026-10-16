//! Empirical Re-verification Challenge Harness for Milestone 3 (R3 & R4)
//! Developed by challenger_m3_r2 to stress-test terminal detection precision,
//! child process cleanup, and focus delay calibration.

use clipboard_history_core::config::PasteConfig;
use clipboard_history_core::injector::{InjectorCascade, PasteInjector};
use clipboard_history_core::window_focus::WindowFocusDetector;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;

// ============================================================================
// 1. Empirical Terminal Detection: False Positive Stress Test
// ============================================================================

#[test]
fn test_adversarial_m3_r2_false_positive_candidates_rejection() {
    // Mandated false-positive candidates:
    // steam, postman, webstorm, rustrover, obs-studio, distrobox, fastfetch
    let mandated_candidates = [
        "steam",
        "postman",
        "webstorm",
        "rustrover",
        "obs-studio",
        "distrobox",
        "fastfetch",
    ];

    for candidate in &mandated_candidates {
        // Test lower, upper, and title-case variants
        let lower = candidate.to_lowercase();
        let upper = candidate.to_uppercase();
        let mut title_case = lower.clone();
        if let Some(first) = title_case.get_mut(0..1) {
            first.make_ascii_uppercase();
        }

        for cls in &[&lower, &upper, &title_case] {
            assert!(
                !WindowFocusDetector::is_terminal_window("Standard Window", cls),
                "False-positive candidate '{}' must NOT be classified as terminal",
                cls
            );
            // Window title mentioning terminal or console
            assert!(
                !WindowFocusDetector::is_terminal_window("Terminal Guide", cls),
                "False-positive candidate '{}' with title 'Terminal Guide' must NOT be terminal",
                cls
            );
            assert!(
                !WindowFocusDetector::is_terminal_window("Console Output", cls),
                "False-positive candidate '{}' with title 'Console Output' must NOT be terminal",
                cls
            );
        }
    }
}

#[test]
fn test_adversarial_m3_r2_short_substring_st_isolation() {
    // Extensive list of words containing "st" that are NOT terminal emulators
    let st_words = [
        "steam",
        "postman",
        "webstorm",
        "rustrover",
        "obs-studio",
        "distrobox",
        "fastfetch",
        "ghostwriter",
        "calibre",
        "wireshark",
        "insomnia",
        "installer",
        "artist",
        "status",
        "firstapp",
        "lastpass",
        "rustup",
        "systemd-manager",
        "storage-explorer",
        "test-runner",
        "station-player",
        "bootstrap",
        "crystal",
        "fastlane",
        "sublime_text",
    ];

    let mut wrongly_detected = Vec::new();
    for word in &st_words {
        if WindowFocusDetector::is_terminal_window("App Window", word) {
            wrongly_detected.push(*word);
        }
    }

    assert!(
        wrongly_detected.is_empty(),
        "Words containing 'st' incorrectly classified as terminal emulators: {:?}",
        wrongly_detected
    );
}

#[test]
fn test_adversarial_m3_r2_short_terminals_boundary_delimiters() {
    // "st" (len 2)
    assert!(WindowFocusDetector::is_terminal_window("", "st"));
    assert!(WindowFocusDetector::is_terminal_window("", "ST"));
    assert!(WindowFocusDetector::is_terminal_window("", "st-256color"));
    assert!(WindowFocusDetector::is_terminal_window("", "st-terminal"));
    assert!(WindowFocusDetector::is_terminal_window("", "simple-st"));
    assert!(WindowFocusDetector::is_terminal_window(
        "",
        "org.suckless.st"
    ));
    assert!(WindowFocusDetector::is_terminal_window("", "st.desktop"));

    // "st" must NOT match arbitrary prefixes/suffixes without delimiters
    assert!(!WindowFocusDetector::is_terminal_window("", "myst"));
    assert!(!WindowFocusDetector::is_terminal_window("", "stable"));
    assert!(!WindowFocusDetector::is_terminal_window("", "custom"));
    assert!(!WindowFocusDetector::is_terminal_window("", "star"));

    // "rio" (len 3)
    assert!(WindowFocusDetector::is_terminal_window("", "rio"));
    assert!(WindowFocusDetector::is_terminal_window("", "RIO"));
    assert!(WindowFocusDetector::is_terminal_window("", "rio-terminal"));
    assert!(WindowFocusDetector::is_terminal_window("", "org.rio"));
    assert!(!WindowFocusDetector::is_terminal_window("", "mario"));
    assert!(!WindowFocusDetector::is_terminal_window("", "priority"));
    assert!(!WindowFocusDetector::is_terminal_window("", "period"));

    // "kgx" (len 3)
    assert!(WindowFocusDetector::is_terminal_window("", "kgx"));
    assert!(WindowFocusDetector::is_terminal_window("", "KGX"));
    assert!(WindowFocusDetector::is_terminal_window(
        "",
        "org.gnome.Console.kgx"
    ));
    assert!(!WindowFocusDetector::is_terminal_window("", "pkgx"));
}

#[test]
fn test_adversarial_m3_r2_jetbrains_family_rejection() {
    let jetbrains_classes = [
        "jetbrains-idea",
        "jetbrains-idea-ce",
        "jetbrains-pycharm",
        "jetbrains-pycharm-ce",
        "jetbrains-webstorm",
        "jetbrains-rustrover",
        "jetbrains-clion",
        "jetbrains-goland",
        "jetbrains-rider",
        "jetbrains-fleet",
        "jetbrains-datagrip",
        "com.jetbrains.rustrover",
        "com.jetbrains.webstorm",
    ];

    for jb in &jetbrains_classes {
        assert!(
            !WindowFocusDetector::is_terminal_window("Embedded Terminal Window", jb),
            "JetBrains app '{}' must NOT be classified as terminal emulator",
            jb
        );
        assert!(
            !WindowFocusDetector::is_terminal_window("Terminal - my-rust-app", jb),
            "JetBrains app '{}' with terminal title must NOT be terminal",
            jb
        );
    }
}

// ============================================================================
// 2. Empirical Terminal Detection: True Positive Legitimate Terminals
// ============================================================================

#[test]
fn test_adversarial_m3_r2_legitimate_terminals_mandatory_list() {
    // Mandated terminals:
    // alacritty, kitty, foot, konsole, gnome-terminal, xterm, wezterm, st-256color, st
    let mandated_terminals = [
        "alacritty",
        "kitty",
        "foot",
        "konsole",
        "gnome-terminal",
        "xterm",
        "wezterm",
        "st-256color",
        "st",
    ];

    for term in &mandated_terminals {
        // Lowercase class
        assert!(
            WindowFocusDetector::is_terminal_window("", term),
            "Mandated terminal '{}' (lower) must evaluate to true",
            term
        );
        // Uppercase class
        assert!(
            WindowFocusDetector::is_terminal_window("", &term.to_uppercase()),
            "Mandated terminal '{}' (upper) must evaluate to true",
            term
        );
        // With realistic shell title
        let title = format!("user@devbox: ~ ({})", term);
        assert!(
            WindowFocusDetector::is_terminal_window(&title, term),
            "Mandated terminal '{}' with realistic title must evaluate to true",
            term
        );
    }
}

#[test]
fn test_adversarial_m3_r2_extended_terminal_catalog() {
    let catalog = [
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

    for term in &catalog {
        assert!(
            WindowFocusDetector::is_terminal_window("", term),
            "Terminal emulator '{}' from catalog must evaluate to true",
            term
        );
    }
}

#[test]
fn test_adversarial_m3_r2_x11_wm_class_space_separated_tokens() {
    // Under X11, WM_CLASS often has: "instance class" e.g. "Alacritty alacritty"
    let x11_wm_classes = [
        ("alacritty", "Alacritty alacritty"),
        ("kitty", "kitty kitty"),
        ("st", "st st-256color"),
        ("xterm", "xterm XTerm"),
        ("foot", "foot foot"),
        ("konsole", "konsole konsole"),
        ("gnome-terminal", "gnome-terminal-server Gnome-terminal"),
        ("wezterm", "org.wezfurlong.wezterm wezterm-gui"),
    ];

    for (name, wm_class) in &x11_wm_classes {
        assert!(
            WindowFocusDetector::is_terminal_window("", wm_class),
            "Terminal '{}' with X11 space-separated WM_CLASS '{}' must evaluate to true",
            name,
            wm_class
        );
    }
}

#[test]
fn test_adversarial_m3_r2_title_only_terminal_fallback() {
    // When class is empty or unavailable, title-based fallback
    let title_terminals = [
        "alacritty",
        "kitty",
        "foot",
        "konsole",
        "gnome-terminal",
        "xterm",
        "wezterm",
        "st",
        "st-256color",
    ];

    for term in &title_terminals {
        // Exact title
        assert!(
            WindowFocusDetector::is_terminal_window(term, ""),
            "Title-only exact match '{}' must evaluate to true",
            term
        );
        // Suffix " - <term>"
        let full_title = format!("user@laptop: ~/src - {}", term);
        assert!(
            WindowFocusDetector::is_terminal_window(&full_title, ""),
            "Title-only suffix match '{}' must evaluate to true",
            full_title
        );
    }

    // Title-only must NOT match generic GUI apps when class is empty
    assert!(!WindowFocusDetector::is_terminal_window("Steam", ""));
    assert!(!WindowFocusDetector::is_terminal_window(
        "Mozilla Firefox",
        ""
    ));
    assert!(!WindowFocusDetector::is_terminal_window("Postman", ""));
    assert!(!WindowFocusDetector::is_terminal_window("WebStorm", ""));
    assert!(!WindowFocusDetector::is_terminal_window("Terminal", "")); // generic word without binary suffix
}

// ============================================================================
// 3. Trait Default Method & Injector Cascade Verification
// ============================================================================

struct DummyInjector {
    recorded_context: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl PasteInjector for DummyInjector {
    fn name(&self) -> &'static str {
        "dummy"
    }
    fn is_available(&self) -> bool {
        true
    }
    async fn inject_context(&self, is_terminal: bool) -> bool {
        self.recorded_context
            .store(is_terminal, std::sync::atomic::Ordering::SeqCst);
        true
    }
}

#[tokio::test]
async fn test_adversarial_m3_r2_paste_injector_trait_no_recursion() {
    let dummy = DummyInjector {
        recorded_context: std::sync::atomic::AtomicBool::new(true),
    };

    // Calling inject() must delegate to inject_context(false) without infinite recursion
    let success = dummy.inject().await;
    assert!(success);
    assert!(
        !dummy
            .recorded_context
            .load(std::sync::atomic::Ordering::SeqCst),
        "inject() must delegate to inject_context(false)"
    );

    // Calling inject_context(true)
    let success_term = dummy.inject_context(true).await;
    assert!(success_term);
    assert!(
        dummy
            .recorded_context
            .load(std::sync::atomic::Ordering::SeqCst),
        "inject_context(true) must record true"
    );
}

#[tokio::test]
async fn test_adversarial_m3_r2_cascade_context_execution() {
    let cascade = InjectorCascade::new();

    // Verify terminal context execution
    let (name_term, ok_term) = cascade.execute_paste_context(true).await;
    assert!(ok_term, "execute_paste_context(true) must succeed");
    assert!(!name_term.is_empty());

    // Verify GUI context execution
    let (name_gui, ok_gui) = cascade.execute_paste_context(false).await;
    assert!(ok_gui, "execute_paste_context(false) must succeed");
    assert_eq!(
        name_term, name_gui,
        "Both contexts must resolve same available injector"
    );
}

// ============================================================================
// 4. Wayland Child Process Safety & Zombie Lingering Stress Tests
// ============================================================================

#[tokio::test]
async fn test_adversarial_m3_r2_child_process_kill_on_drop_single() {
    let mut cmd = tokio::process::Command::new("sleep");
    cmd.arg("60");
    cmd.kill_on_drop(true);

    let child = cmd.spawn().expect("failed to spawn sleep");
    let pid = child.id().expect("must have pid") as i32;

    // Verify initially alive
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0, "Process must be alive");

    // Drop child
    drop(child);

    // Wait 100ms for tokio driver to kill and reap
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check proc status
    let proc_status = format!("/proc/{}/status", pid);
    if Path::new(&proc_status).exists() {
        let content = fs::read_to_string(&proc_status).unwrap_or_default();
        let is_zombie = content
            .lines()
            .any(|l| l.starts_with("State:") && l.contains('Z'));
        assert!(!is_zombie, "Process {} became zombie: {}", pid, content);
    }

    // Must be completely gone
    assert_ne!(
        unsafe { libc::kill(pid, 0) },
        0,
        "Process {} still alive after kill_on_drop!",
        pid
    );
}

#[tokio::test]
async fn test_adversarial_m3_r2_stress_rapid_timeouts_no_zombies() {
    // 50 sequential rapid timeouts
    let mut pids = Vec::with_capacity(50);

    for _ in 0..50 {
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("30");
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().expect("spawn child");
        let pid = child.id().expect("pid") as i32;
        pids.push(pid);

        // Rapid timeout 5ms
        let _ = timeout(Duration::from_millis(5), child.wait()).await;
        drop(child);
    }

    // Reaping window
    tokio::time::sleep(Duration::from_millis(250)).await;

    let mut zombies = Vec::new();
    let mut alive = Vec::new();

    for pid in pids {
        let stat_path = format!("/proc/{}/status", pid);
        if Path::new(&stat_path).exists() {
            let content = fs::read_to_string(&stat_path).unwrap_or_default();
            if content
                .lines()
                .any(|l| l.starts_with("State:") && l.contains('Z'))
            {
                zombies.push(pid);
            } else if unsafe { libc::kill(pid, 0) == 0 } {
                alive.push(pid);
            }
        }
    }

    assert!(zombies.is_empty(), "Lingering zombies: {:?}", zombies);
    assert!(alive.is_empty(), "Lingering alive: {:?}", alive);
}

#[tokio::test]
async fn test_adversarial_m3_r2_concurrent_timeouts_no_zombies() {
    let handles: Vec<_> = (0..20)
        .map(|_| {
            tokio::spawn(async {
                let mut cmd = tokio::process::Command::new("sleep");
                cmd.arg("20");
                cmd.kill_on_drop(true);

                let mut child = cmd.spawn().expect("spawn");
                let pid = child.id().expect("pid") as i32;

                let _ = timeout(Duration::from_millis(10), child.wait()).await;
                drop(child);
                pid
            })
        })
        .collect();

    let mut pids = Vec::new();
    for h in handles {
        pids.push(h.await.unwrap());
    }

    tokio::time::sleep(Duration::from_millis(250)).await;

    let mut zombies = Vec::new();
    let mut alive = Vec::new();

    for pid in pids {
        let stat_path = format!("/proc/{}/status", pid);
        if Path::new(&stat_path).exists() {
            let content = fs::read_to_string(&stat_path).unwrap_or_default();
            if content
                .lines()
                .any(|l| l.starts_with("State:") && l.contains('Z'))
            {
                zombies.push(pid);
            } else if unsafe { libc::kill(pid, 0) == 0 } {
                alive.push(pid);
            }
        }
    }

    assert!(zombies.is_empty(), "Concurrent zombies: {:?}", zombies);
    assert!(alive.is_empty(), "Concurrent alive: {:?}", alive);
}

// ============================================================================
// 5. Focus Delay Calibration Bounds & Wayland Minimum
// ============================================================================

#[test]
fn test_adversarial_m3_r2_focus_delay_calibration() {
    let mut cfg = PasteConfig::default();

    // Default config
    assert!(cfg.paste_delay_ms >= 50);

    // Test with Wayland env simulated
    std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
    std::env::set_var("XDG_SESSION_TYPE", "wayland");

    cfg.paste_delay_ms = 0;
    assert_eq!(
        cfg.effective_delay_ms(),
        120,
        "Wayland 0ms must clamp to 120ms"
    );

    cfg.paste_delay_ms = 30;
    assert_eq!(
        cfg.effective_delay_ms(),
        120,
        "Wayland 30ms must clamp to 120ms"
    );

    cfg.paste_delay_ms = 119;
    assert_eq!(
        cfg.effective_delay_ms(),
        120,
        "Wayland 119ms must clamp to 120ms"
    );

    cfg.paste_delay_ms = 120;
    assert_eq!(
        cfg.effective_delay_ms(),
        120,
        "Wayland 120ms must remain 120ms"
    );

    cfg.paste_delay_ms = 350;
    assert_eq!(
        cfg.effective_delay_ms(),
        350,
        "Wayland 350ms must remain 350ms"
    );

    // Test with X11 env simulated
    std::env::remove_var("WAYLAND_DISPLAY");
    std::env::set_var("XDG_SESSION_TYPE", "x11");

    cfg.paste_delay_ms = 0;
    assert_eq!(cfg.effective_delay_ms(), 50, "X11 0ms must clamp to 50ms");

    cfg.paste_delay_ms = 25;
    assert_eq!(cfg.effective_delay_ms(), 50, "X11 25ms must clamp to 50ms");

    cfg.paste_delay_ms = 50;
    assert_eq!(cfg.effective_delay_ms(), 50, "X11 50ms must remain 50ms");

    cfg.paste_delay_ms = 200;
    assert_eq!(cfg.effective_delay_ms(), 200, "X11 200ms must remain 200ms");

    // Clean up env
    std::env::remove_var("WAYLAND_DISPLAY");
    std::env::remove_var("XDG_SESSION_TYPE");
}
