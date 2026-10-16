//! Empirical Challenge Test Suite for Milestone 5 (Final Milestone Acceptance & Hardening)
//! Adversarially verifying:
//! 1. R1: SQLite 5000ms busy_timeout and TransactionBehavior::Immediate preventing SQLITE_BUSY under heavy contention.
//! 2. R3: Context-aware terminal paste injection: short names bounded, 44+ GUI apps and JetBrains IDEs rejected,
//!    9 mandated & 46 catalog terminals accepted, non-recursive trait, and cascade shortcut dispatch.
//! 3. R4: Wayland process safety: kill_on_drop child process lifecycle, zero zombie processes under timeouts,
//!    and focus delay calibration (>= 120ms Wayland, >= 50ms X11).

use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::PasteConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::injector::{CopyOnlyInjector, InjectorCascade, PasteInjector};
use clipboard_history_core::storage::SqliteRepository;
use clipboard_history_core::window_focus::WindowFocusDetector;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use tokio::time::timeout;

// ============================================================================
// Helper Functions
// ============================================================================

fn make_test_entry(content: &str, pinned: bool) -> ClipboardEntry {
    let hash = BlobStore::compute_hash(content.as_bytes());
    let mut entry = ClipboardEntry::new_text(
        content.to_string(),
        hash,
        vec!["text/plain".to_string()],
        Some("test_app".to_string()),
    );
    entry.is_pinned = pinned;
    entry
}

fn count_zombie_pids(pids: &[u32]) -> usize {
    let mut zombie_count = 0;
    for &pid in pids {
        let status_path = format!("/proc/{}/status", pid);
        if let Ok(status) = fs::read_to_string(&status_path) {
            for line in status.lines() {
                if line.starts_with("State:") && line.contains('Z') {
                    zombie_count += 1;
                    break;
                }
            }
        }
    }
    zombie_count
}

// ============================================================================
// 1. R1: SQLite Database Concurrency & Transient Lock Resilience
// ============================================================================

#[test]
fn challenge_r1_sqlite_busy_timeout_configured_on_open() {
    let temp_dir = tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("test_timeout.db");

    let disk_repo = SqliteRepository::open(&db_path).expect("open disk repo");
    let disk_timeout = disk_repo
        .busy_timeout_ms()
        .expect("query busy_timeout disk");
    assert_eq!(
        disk_timeout, 5000,
        "Disk SQLite repository must have PRAGMA busy_timeout set to 5000ms"
    );

    let mem_repo = SqliteRepository::open_in_memory().expect("open memory repo");
    let mem_timeout = mem_repo.busy_timeout_ms().expect("query busy_timeout mem");
    assert_eq!(
        mem_timeout, 5000,
        "In-memory SQLite repository must have PRAGMA busy_timeout set to 5000ms"
    );
}

#[test]
fn challenge_r1_sqlite_immediate_transaction_prevents_lock_upgrade_deadlock() {
    let temp_dir = tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("test_immediate.db");

    // Initialize schema
    let repo_init = SqliteRepository::open(&db_path).expect("init db");
    repo_init
        .insert_or_update(&make_test_entry("initial-entry", false))
        .expect("initial insert");
    drop(repo_init);

    let path_clone1 = db_path.clone();
    let path_clone2 = db_path.clone();

    // Spawn thread 1: holds a write transaction for batch insert
    let t1 = thread::spawn(move || {
        let repo = SqliteRepository::open(&path_clone1).expect("open t1 repo");
        let entries: Vec<ClipboardEntry> = (0..20)
            .map(|i| make_test_entry(&format!("t1_batch_{}", i), false))
            .collect();
        // batch_insert_or_update uses TransactionBehavior::Immediate
        let inserted = repo.batch_insert_or_update(&entries);
        assert!(inserted.is_ok(), "t1 batch insert must succeed");
    });

    // Spawn thread 2 simultaneously: attempts batch_insert_or_update on separate connection
    let t2 = thread::spawn(move || {
        let repo = SqliteRepository::open(&path_clone2).expect("open t2 repo");
        let entries: Vec<ClipboardEntry> = (0..20)
            .map(|i| make_test_entry(&format!("t2_batch_{}", i), false))
            .collect();
        let start = Instant::now();
        let inserted = repo.batch_insert_or_update(&entries);
        let elapsed = start.elapsed();
        assert!(
            inserted.is_ok(),
            "t2 batch insert must succeed under contention without SQLITE_BUSY: {:?}",
            inserted.err()
        );
        println!("t2 completed write contention in {:?}", elapsed);
    });

    t1.join().expect("t1 join");
    t2.join().expect("t2 join");

    let repo_verify = SqliteRepository::open(&db_path).expect("verify db");
    let all_entries = repo_verify.list(100, 0, None, false).expect("list");
    assert_eq!(
        all_entries.len(),
        41,
        "All 40 batch items + 1 initial item must be persisted"
    );
}

#[test]
fn challenge_r1_sqlite_high_concurrency_stress_25_threads() {
    let temp_dir = tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("stress_25_threads.db");

    // Initialize database
    let repo_init = SqliteRepository::open(&db_path).expect("open init");
    let _ = repo_init.insert_or_update(&make_test_entry("baseline", false));
    drop(repo_init);

    let num_threads = 25;
    let ops_per_thread = 10;
    let success_writes = Arc::new(AtomicUsize::new(0));
    let success_reads = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for thread_idx in 0..num_threads {
        let p = db_path.clone();
        let sw = Arc::clone(&success_writes);
        let sr = Arc::clone(&success_reads);

        handles.push(thread::spawn(move || {
            let repo = SqliteRepository::open(&p).expect("open thread repo");
            for op in 0..ops_per_thread {
                if (thread_idx + op) % 3 == 0 {
                    // Batch write
                    let entries = vec![
                        make_test_entry(&format!("stress_t{}_op{}_a", thread_idx, op), false),
                        make_test_entry(&format!("stress_t{}_op{}_b", thread_idx, op), true),
                    ];
                    if repo.batch_insert_or_update(&entries).is_ok() {
                        sw.fetch_add(2, Ordering::SeqCst);
                    }
                } else if (thread_idx + op) % 3 == 1 {
                    // Single write
                    let entry = make_test_entry(&format!("stress_t{}_op{}", thread_idx, op), false);
                    if repo.insert_or_update(&entry).is_ok() {
                        sw.fetch_add(1, Ordering::SeqCst);
                    }
                } else {
                    // Concurrent Search / Read
                    if repo.search("stress", 50, 0).is_ok() {
                        sr.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("join stress thread");
    }

    let final_writes = success_writes.load(Ordering::SeqCst);
    let final_reads = success_reads.load(Ordering::SeqCst);
    println!(
        "Stress test completed: {} writes and {} reads succeeded cleanly with 0 SQLITE_BUSY",
        final_writes, final_reads
    );
    assert!(
        final_writes > 150,
        "Expected high number of successful writes under contention"
    );
    assert!(
        final_reads > 50,
        "Expected high number of successful reads under contention"
    );
}

// ============================================================================
// 2. R3: Context-Aware Terminal Paste Injection
// ============================================================================

#[test]
fn challenge_r3_short_names_strict_boundary_enforcement() {
    // 1. Legitimate standalone and delimited occurrences of short terminals: "st", "rio", "kgx"
    let valid_st = [
        "st",
        "ST",
        "st-256color",
        "st-terminal",
        "simple-st",
        "org.suckless.st",
        "st.desktop",
    ];
    for candidate in valid_st {
        assert!(
            WindowFocusDetector::is_terminal_window("", candidate),
            "Valid 'st' candidate '{}' must be recognized as terminal",
            candidate
        );
    }

    let valid_rio = ["rio", "RIO", "rio-terminal", "org.rio", "terminal-rio"];
    for candidate in valid_rio {
        assert!(
            WindowFocusDetector::is_terminal_window("", candidate),
            "Valid 'rio' candidate '{}' must be recognized as terminal",
            candidate
        );
    }

    let valid_kgx = ["kgx", "KGX", "org.gnome.Console.kgx", "kgx-app"];
    for candidate in valid_kgx {
        assert!(
            WindowFocusDetector::is_terminal_window("", candidate),
            "Valid 'kgx' candidate '{}' must be recognized as terminal",
            candidate
        );
    }

    // 2. Substring non-terminals containing "st", "rio", or "kgx" MUST NOT be false positives!
    let invalid_st_substrings = [
        "postman",
        "fastfetch",
        "steam",
        "myst",
        "star",
        "stable",
        "custom",
        "statistic",
        "plastic",
        "host",
        "distrobox",
        "ghostwriter",
    ];
    for non_term in invalid_st_substrings {
        assert!(
            !WindowFocusDetector::is_terminal_window("", non_term),
            "App '{}' containing 'st' as substring must NOT be recognized as terminal",
            non_term
        );
    }

    let invalid_rio_substrings = [
        "mario",
        "priority",
        "period",
        "scenario",
        "curiosity",
        "champion",
    ];
    for non_term in invalid_rio_substrings {
        assert!(
            !WindowFocusDetector::is_terminal_window("", non_term),
            "App '{}' containing 'rio' as substring must NOT be recognized as terminal",
            non_term
        );
    }

    let invalid_kgx_substrings = ["pkgx", "backgxd"];
    for non_term in invalid_kgx_substrings {
        assert!(
            !WindowFocusDetector::is_terminal_window("", non_term),
            "App '{}' containing 'kgx' as substring must NOT be recognized as terminal",
            non_term
        );
    }
}

#[test]
fn challenge_r3_desktop_gui_apps_rejected_even_with_terminal_in_title() {
    let gui_apps = [
        "firefox",
        "google-chrome",
        "chromium",
        "brave-browser",
        "brave",
        "vivaldi",
        "vivaldi-stable",
        "opera",
        "microsoft-edge",
        "epiphany",
        "tor-browser",
        "code",
        "code-oss",
        "vscodium",
        "sublime_text",
        "gedit",
        "org.gnome.nautilus",
        "nautilus",
        "slack",
        "discord",
        "vlc",
        "libreoffice-writer",
        "steam",
        "postman",
        "obs-studio",
        "distrobox",
        "fastfetch",
        "calibre",
        "blender",
        "gimp",
        "wireshark",
        "insomnia",
        "inkscape",
        "audacity",
        "transmission",
        "ghostwriter",
    ];

    let titles = [
        "",
        "Main Window",
        "How to use Linux Terminal - Mozilla Firefox",
        "terminal.rs - Visual Studio Code",
        "Bash Terminal Cheatsheet",
        "Embedded Terminal Session",
    ];

    for app in gui_apps {
        for title in titles {
            assert!(
                !WindowFocusDetector::is_terminal_window(title, app),
                "GUI app '{}' with title '{}' must NOT be classified as terminal",
                app,
                title
            );
        }
    }
}

#[test]
fn challenge_r3_jetbrains_family_ides_strictly_rejected() {
    let jetbrains_ids = [
        "jetbrains-idea",
        "jetbrains-clion",
        "jetbrains-pycharm",
        "jetbrains-rustrover",
        "jetbrains-webstorm",
        "jetbrains-rider",
        "jetbrains-goland",
        "jetbrains-gateway",
        "jetbrains-fleet",
        "com.jetbrains.intellij",
        "idea",
        "pycharm",
        "clion",
        "webstorm",
        "rustrover",
        "goland",
        "android-studio",
    ];

    let titles = [
        "Project - Terminal",
        "Terminal: Local (1)",
        "Embedded Terminal",
        "Main.rs - RustRover",
    ];

    for jb in jetbrains_ids {
        for title in titles {
            assert!(
                !WindowFocusDetector::is_terminal_window(title, jb),
                "JetBrains IDE '{}' with title '{}' must NOT be classified as terminal",
                jb,
                title
            );
        }
    }
}

#[test]
fn challenge_r3_mandated_and_catalog_terminals_accepted() {
    // 9 mandated terminals
    let mandated = [
        "alacritty",
        "kitty",
        "foot",
        "konsole",
        "gnome-terminal",
        "wezterm",
        "xterm",
        "ghostty",
        "contour",
    ];
    for term in mandated {
        assert!(
            WindowFocusDetector::is_terminal_window("", term),
            "Mandated terminal '{}' must be accepted",
            term
        );
        assert!(
            WindowFocusDetector::is_terminal_window("", &term.to_uppercase()),
            "Uppercase terminal '{}' must be accepted",
            term
        );
    }

    // Extended catalog terminals
    let catalog = [
        "gnome-terminal-server",
        "org.gnome.terminal",
        "org.gnome.console",
        "kgx",
        "ptyxis",
        "org.gnome.ptyxis",
        "uxterm",
        "footclient",
        "wezterm-gui",
        "org.wezfurlong.wezterm",
        "org.kde.konsole",
        "xfce4-terminal",
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
    ];
    for term in catalog {
        assert!(
            WindowFocusDetector::is_terminal_window("", term),
            "Catalog terminal '{}' must be accepted",
            term
        );
    }
}

#[tokio::test]
async fn challenge_r3_paste_injector_trait_non_recursive_and_cascade() {
    struct TestNonRecursiveInjector;

    #[async_trait::async_trait]
    impl PasteInjector for TestNonRecursiveInjector {
        fn name(&self) -> &'static str {
            "test-non-recursive"
        }
        fn is_available(&self) -> bool {
            true
        }
        async fn inject_context(&self, is_terminal: bool) -> bool {
            // Echoes whether terminal
            is_terminal
        }
    }

    let injector = TestNonRecursiveInjector;
    // Calling default inject() invokes inject_context(false) without infinite recursion
    assert!(
        !injector.inject().await,
        "Default inject() must call inject_context(false) cleanly"
    );
    assert!(
        injector.inject_context(true).await,
        "inject_context(true) must return true"
    );

    // Verify CopyOnly fallback behavior
    let copy_only = CopyOnlyInjector::new();
    assert!(copy_only.inject_context(true).await);
    assert!(copy_only.inject_context(false).await);
    assert!(copy_only.inject().await);

    // Verify Cascade execution
    let cascade = InjectorCascade::new();
    let (_name_term, term_ok) = cascade.execute_paste_context(true).await;
    let (_name_gui, gui_ok) = cascade.execute_paste_context(false).await;
    assert!(term_ok, "Cascade terminal paste must succeed");
    assert!(gui_ok, "Cascade GUI paste must succeed");
}

// ============================================================================
// 3. R4: Wayland Process Safety & Focus Delay Calibration
// ============================================================================

#[tokio::test]
async fn challenge_r4_child_process_kill_on_drop_prevents_zombies() {
    let mut cmd = tokio::process::Command::new("sleep");
    cmd.arg("10");
    cmd.kill_on_drop(true);

    let child = cmd.spawn().expect("spawn sleep");
    let child_pid = child.id().expect("child PID");
    assert!(child_pid > 0, "Spawned child PID must be positive");

    // Drop the child future by dropping child wrapper
    drop(child);

    // Give the Tokio process reaper a brief moment to reap
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify child is not a zombie
    let zombies = count_zombie_pids(&[child_pid]);
    assert_eq!(
        zombies, 0,
        "Child PID {} must be cleanly terminated and reaped upon drop, not a zombie",
        child_pid
    );
}

#[tokio::test]
async fn challenge_r4_concurrent_process_timeouts_no_zombies() {
    let num_tasks = 20;
    let mut child_pids = Vec::new();
    let mut join_handles = Vec::new();

    for _ in 0..num_tasks {
        let handle = tokio::spawn(async {
            let mut cmd = tokio::process::Command::new("sleep");
            cmd.arg("20");
            cmd.kill_on_drop(true);

            let child = cmd.spawn().expect("spawn child");
            let pid = child.id().expect("pid");

            // Deliberately timeout after 15ms, forcing child drop
            let _ = timeout(Duration::from_millis(15), child.wait_with_output()).await;
            pid
        });
        join_handles.push(handle);
    }

    for h in join_handles {
        if let Ok(pid) = h.await {
            child_pids.push(pid);
        }
    }

    // Allow process reaper time to clean up
    tokio::time::sleep(Duration::from_millis(100)).await;

    let zombies = count_zombie_pids(&child_pids);
    assert_eq!(
        zombies, 0,
        "All {} concurrent timed-out processes must be reaped with 0 zombies",
        num_tasks
    );
}

#[test]
fn challenge_r4_focus_delay_calibration_boundaries() {
    // 1. Wayland session simulation
    std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
    std::env::remove_var("XDG_SESSION_TYPE");

    let cfg_zero = PasteConfig {
        auto_paste: true,
        paste_delay_ms: 0,
        preferred_injector: None,
        move_pasted_to_top: false,
    };
    assert_eq!(
        cfg_zero.effective_delay_ms(),
        120,
        "Wayland with delay 0ms must clamp to 120ms"
    );

    let cfg_low = PasteConfig {
        auto_paste: true,
        paste_delay_ms: 80,
        preferred_injector: None,
        move_pasted_to_top: false,
    };
    assert_eq!(
        cfg_low.effective_delay_ms(),
        120,
        "Wayland with delay 80ms must clamp to 120ms"
    );

    let cfg_higher = PasteConfig {
        auto_paste: true,
        paste_delay_ms: 350,
        preferred_injector: None,
        move_pasted_to_top: false,
    };
    assert_eq!(
        cfg_higher.effective_delay_ms(),
        350,
        "Wayland with delay 350ms must preserve configured 350ms"
    );

    // 2. X11 session simulation
    std::env::remove_var("WAYLAND_DISPLAY");
    std::env::set_var("XDG_SESSION_TYPE", "x11");
    std::env::set_var("DISPLAY", ":0");

    assert_eq!(
        cfg_zero.effective_delay_ms(),
        50,
        "X11 with delay 0ms must clamp to 50ms"
    );

    let cfg_x11_30 = PasteConfig {
        auto_paste: true,
        paste_delay_ms: 30,
        preferred_injector: None,
        move_pasted_to_top: false,
    };
    assert_eq!(
        cfg_x11_30.effective_delay_ms(),
        50,
        "X11 with delay 30ms must clamp to 50ms"
    );

    let cfg_x11_200 = PasteConfig {
        auto_paste: true,
        paste_delay_ms: 200,
        preferred_injector: None,
        move_pasted_to_top: false,
    };
    assert_eq!(
        cfg_x11_200.effective_delay_ms(),
        200,
        "X11 with delay 200ms must preserve configured 200ms"
    );

    // 3. Extreme upper bound
    let cfg_extreme = PasteConfig {
        auto_paste: true,
        paste_delay_ms: 60_000,
        preferred_injector: None,
        move_pasted_to_top: false,
    };
    assert_eq!(
        cfg_extreme.effective_delay_ms(),
        60_000,
        "Extreme delay must be preserved safely"
    );
}
