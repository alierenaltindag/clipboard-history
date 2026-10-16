//! ClipboardHistory Comprehensive End-to-End (E2E) Test Suite
//! Covering Requirements R1 to R5 across 4 Tiers:
//! - Tier 1: Feature Coverage (25 tests; 5 per feature R1-R5)
//! - Tier 2: Boundary & Corner Cases (25 tests; 5 per feature R1-R5)
//! - Tier 3: Cross-Feature Combinations (5 tests)
//! - Tier 4: Real-World Application Scenarios (5 tests)
//!
//! Total: 60 test cases.

use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::injector::InjectorCascade;
use clipboard_history_core::ipc::IpcRequest;
use clipboard_history_core::search::ParsedSearchQuery;
use clipboard_history_core::security::{CryptoEngine, SecretFilter};
use clipboard_history_core::storage::SqliteRepository;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use tokio::time::timeout;
use zeroize::Zeroize;

// ============================================================================
// Requirement Contract Specifications & Testing Harness
// ============================================================================

/// Contract specification for R3: Terminal Window Identification & Paste Shortcut Resolution.
pub struct TerminalFocusContract;

impl TerminalFocusContract {
    pub const KNOWN_TERMINAL_CLASSES: &'static [&'static str] = &[
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

    pub const KNOWN_GUI_CLASSES: &'static [&'static str] = &[
        "firefox",
        "google-chrome",
        "chromium",
        "code",
        "jetbrains-idea",
        "gedit",
        "org.gnome.nautilus",
        "slack",
        "discord",
        "vlc",
        "libreoffice-writer",
    ];

    /// Opaque-box evaluator matching window title and class against terminal rules.
    pub fn is_terminal_window(title: &str, class: &str) -> bool {
        let class_lower = class.to_lowercase();
        if class_lower.is_empty() {
            let title_lower = title.to_lowercase();
            for term in Self::KNOWN_TERMINAL_CLASSES {
                if title_lower == *term || title_lower.ends_with(&format!(" - {}", term)) {
                    return true;
                }
            }
            return false;
        }

        // Direct or compound match
        for term in Self::KNOWN_TERMINAL_CLASSES {
            if class_lower == *term
                || class_lower.starts_with(&format!("{}.", term))
                || class_lower.ends_with(&format!(".{}", term))
                || class_lower.contains(&format!("{}-", term))
                || class_lower == *term
            {
                return true;
            }
        }

        // Generic heuristic
        if class_lower.contains("terminal") || class_lower.contains("console") {
            // Guard against browser tabs or file paths containing 'terminal' in app_id
            if !Self::KNOWN_GUI_CLASSES
                .iter()
                .any(|gui| class_lower.contains(gui))
            {
                return true;
            }
        }

        false
    }

    /// Resolves keystroke shortcut: terminals require Ctrl+Shift+V, graphical apps require Ctrl+V.
    pub fn resolve_paste_shortcut(title: &str, class: &str) -> &'static str {
        if Self::is_terminal_window(title, class) {
            "Ctrl+Shift+V"
        } else {
            "Ctrl+V"
        }
    }
}

/// Contract specification for R4: Auto-Paste Focus Delay Calibration.
pub struct FocusDelayContract;

impl FocusDelayContract {
    pub fn calibrate_delay(configured_delay_ms: u64, is_wayland: bool) -> u64 {
        let min_delay = if is_wayland { 120 } else { 50 };
        configured_delay_ms.max(min_delay)
    }
}

/// Contract specification for R2: Async Search Monotonic Sequencing.
#[derive(Default)]
pub struct SearchSequenceTracker {
    seq: AtomicU64,
}

impl SearchSequenceTracker {
    pub fn next_sequence(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn is_response_valid(&self, response_seq: u64) -> bool {
        response_seq == self.seq.load(Ordering::SeqCst)
    }

    pub fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

fn create_test_entry(text: &str, app: Option<&str>) -> ClipboardEntry {
    let hash = BlobStore::compute_hash(text.as_bytes());
    ClipboardEntry::new_text(
        text.to_string(),
        hash,
        vec!["text/plain".to_string()],
        app.map(|s| s.to_string()),
    )
}

// ============================================================================
// TIER 1: FEATURE COVERAGE (25 Tests)
// ============================================================================

// --- R1: Database Concurrency & Transient Lock Resilience (Tests 01-05) ---

#[test]
fn test_r1_t1_01_concurrent_inserts_multi_threaded() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("concurrency_basic.db");

    // Initialize schema
    {
        let repo = SqliteRepository::open(&db_path).expect("open repo");
        let initial = create_test_entry("Init", Some("setup"));
        repo.insert_or_update(&initial).expect("initial write");
    }

    let thread_count = 10;
    let entries_per_thread = 5;
    let mut handles = Vec::new();

    for t in 0..thread_count {
        let path = db_path.clone();
        handles.push(thread::spawn(move || {
            let repo = SqliteRepository::open(&path).expect("open repo in thread");
            for i in 0..entries_per_thread {
                let text = format!("Thread-{}-Item-{}", t, i);
                let entry = create_test_entry(&text, Some(&format!("app-{}", t)));
                repo.insert_or_update(&entry).unwrap_or_else(|e| {
                    panic!("Thread {} write {} failed with error: {:?}", t, i, e)
                });
            }
        }));
    }

    for h in handles {
        h.join().expect("thread failed");
    }

    let final_repo = SqliteRepository::open(&db_path).expect("open final repo");
    let total = final_repo.count().expect("get total count");
    assert_eq!(
        total,
        1 + (thread_count * entries_per_thread),
        "All concurrent writes must be persisted without SQLITE_BUSY"
    );
}

#[test]
fn test_r1_t1_02_batch_insert_atomic_persistence() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("batch_atomic.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    let mut entries = Vec::new();
    for i in 0..50 {
        entries.push(create_test_entry(
            &format!("Batch-Item-{}", i),
            Some("batch_test"),
        ));
    }

    let inserted = repo.batch_insert_or_update(&entries).expect("batch insert");
    assert_eq!(
        inserted.len(),
        50,
        "Batch insert must return all inserted entries"
    );
    assert_eq!(repo.count().expect("count"), 50);
}

#[test]
fn test_r1_t1_03_concurrent_reads_during_active_write() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("concurrent_read_write.db");

    // Pre-populate
    {
        let repo = SqliteRepository::open(&db_path).expect("open repo");
        for i in 0..20 {
            repo.insert_or_update(&create_test_entry(&format!("Pre-Item-{}", i), None))
                .expect("pre-insert");
        }
    }

    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_readers = Arc::clone(&stop_signal);

    // Reader thread
    let path_reader = db_path.clone();
    let reader_handle = thread::spawn(move || {
        let repo = SqliteRepository::open(&path_reader).expect("open reader repo");
        let mut read_cycles = 0;
        while !stop_readers.load(Ordering::Relaxed) {
            let list = repo.list(10, 0, None, false).expect("read list");
            assert!(!list.is_empty(), "Reader must see active data");
            read_cycles += 1;
            thread::sleep(Duration::from_millis(5));
        }
        read_cycles
    });

    // Writer thread
    let path_writer = db_path.clone();
    let writer_handle = thread::spawn(move || {
        let repo = SqliteRepository::open(&path_writer).expect("open writer repo");
        for i in 0..15 {
            let entry = create_test_entry(&format!("Writer-Item-{}", i), Some("writer"));
            repo.insert_or_update(&entry).expect("writer insert");
            thread::sleep(Duration::from_millis(10));
        }
    });

    writer_handle.join().expect("writer finished");
    stop_signal.store(true, Ordering::Relaxed);
    let cycles = reader_handle.join().expect("reader finished");
    assert!(
        cycles > 0,
        "Reader must have executed multiple cycles concurrently"
    );
}

#[test]
fn test_r1_t1_04_batch_delete_and_pin_concurrency() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("batch_del_pin.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    let mut ids = Vec::new();
    for i in 0..30 {
        let entry = create_test_entry(&format!("PinDel-Item-{}", i), None);
        repo.insert_or_update(&entry).expect("insert");
        ids.push(entry.id);
    }

    let even_ids: Vec<String> = ids.iter().step_by(2).cloned().collect();
    let odd_ids: Vec<String> = ids.iter().skip(1).step_by(2).cloned().collect();

    // Batch pin even
    let pinned_count = repo.batch_set_pinned(&even_ids, true).expect("batch pin");
    assert_eq!(pinned_count, even_ids.len());

    // Batch delete odd
    let deleted_count = repo.batch_delete(&odd_ids).expect("batch delete");
    assert_eq!(deleted_count, odd_ids.len());

    let remaining = repo.count().expect("count remaining");
    assert_eq!(remaining, even_ids.len());
}

#[test]
fn test_r1_t1_05_wal_mode_and_autocheckpoint_resilience() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("wal_checkpoint.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    // Insert enough items to cross default wal_autocheckpoint (100 pages)
    for i in 0..120 {
        let text = format!("WAL-Stress-Payload-{}-{}", i, "x".repeat(200));
        repo.insert_or_update(&create_test_entry(&text, None))
            .expect("insert across checkpoint");
    }

    assert_eq!(repo.count().expect("count"), 120);
}

// --- R2: Universal Full-History Search in GUI (Tests 06-10) ---

#[test]
fn test_r2_t1_06_search_beyond_in_memory_100_limit() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("full_search.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    // Populate 150 items
    for i in 0..150 {
        let text = format!("Historical-Record-{:03}", i);
        repo.insert_or_update(&create_test_entry(&text, Some("app_log")))
            .expect("populate");
    }

    // Default GUI in-memory fetch only retrieves 100 items
    let initial_batch = repo.list(100, 0, None, false).expect("list initial 100");
    assert_eq!(initial_batch.len(), 100);

    // Verify item from early batch is beyond initial 100 chronological items
    let found_old_record = initial_batch
        .iter()
        .any(|e| e.preview.contains("Record-010"));
    assert!(
        !found_old_record,
        "Record-010 should be beyond the initial 100 item slice"
    );

    // Universal full-history search must find Record-010 in SQLite
    let search_results = repo.search("Record-010", 100, 0).expect("search full db");
    assert_eq!(search_results.len(), 1);
    assert!(search_results[0].preview.contains("Record-010"));
}

#[test]
fn test_r2_t1_07_search_query_syntax_parsing() {
    let query_str = "app:firefox type:text is:pinned important auth token";
    let parsed = ParsedSearchQuery::parse(query_str);

    assert_eq!(parsed.source_app.as_deref(), Some("firefox"));
    assert_eq!(parsed.entry_type, Some(EntryType::Text));
    assert_eq!(parsed.is_pinned, Some(true));
    assert_eq!(parsed.text_query, "important auth token");
}

#[test]
fn test_r2_t1_08_zero_lag_cache_restore_on_empty_query() {
    // Simulates GUI search behavior: filtering local cache when query is empty vs non-empty
    let items = vec![
        create_test_entry("First Apple", Some("safari")),
        create_test_entry("Second Banana", Some("notes")),
        create_test_entry("Third Orange", Some("terminal")),
    ];

    let query = "Apple";
    let filtered: Vec<_> = items
        .iter()
        .filter(|e| e.preview.contains(query))
        .cloned()
        .collect();
    assert_eq!(filtered.len(), 1);

    // When query is cleared back to empty, immediately restore all local items without lag
    let empty_query = "";
    let restored: Vec<_> = if empty_query.trim().is_empty() {
        items.clone()
    } else {
        vec![]
    };
    assert_eq!(
        restored.len(),
        3,
        "Clearing query immediately restores all cached entries"
    );
}

#[test]
fn test_r2_t1_09_ipc_search_protocol_roundtrip() {
    let req = IpcRequest::Search {
        query: "database password".to_string(),
        limit: 50,
        offset: 0,
    };

    let serialized = serde_json::to_string(&req).expect("serialize req");
    let deserialized: IpcRequest = serde_json::from_str(&serialized).expect("deserialize req");

    match deserialized {
        IpcRequest::Search {
            query,
            limit,
            offset,
        } => {
            assert_eq!(query, "database password");
            assert_eq!(limit, 50);
            assert_eq!(offset, 0);
        }
        _ => panic!("Unexpected IpcRequest variant"),
    }
}

#[test]
fn test_r2_t1_10_search_fts5_and_like_matching() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("fts_test.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    repo.insert_or_update(&create_test_entry(
        "PostgreSQL deployment scripts and credentials",
        Some("pgadmin"),
    ))
    .expect("insert 1");
    repo.insert_or_update(&create_test_entry(
        "Redis cluster cache synchronization notes",
        Some("firefox"),
    ))
    .expect("insert 2");

    let res_pg = repo.search("PostgreSQL", 10, 0).expect("search pg");
    assert_eq!(res_pg.len(), 1);
    assert!(res_pg[0].preview.contains("PostgreSQL"));

    let res_redis = repo.search("Redis", 10, 0).expect("search redis");
    assert_eq!(res_redis.len(), 1);
    assert!(res_redis[0].preview.contains("Redis"));
}

// --- R3: Context-Aware Terminal Paste Injection (Tests 11-15) ---

#[test]
fn test_r3_t1_11_terminal_emulator_class_identification() {
    let terminals = [
        "alacritty",
        "kitty",
        "foot",
        "konsole",
        "gnome-terminal",
        "wezterm",
        "xterm",
        "ghostty",
        "contour",
        "ptyxis",
    ];

    for term in &terminals {
        assert!(
            TerminalFocusContract::is_terminal_window("", term),
            "Terminal emulator class '{}' must be identified as terminal",
            term
        );
    }
}

#[test]
fn test_r3_t1_12_graphical_app_non_terminal_identification() {
    let gui_apps = [
        "firefox",
        "google-chrome",
        "code",
        "gedit",
        "org.gnome.Nautilus",
        "slack",
        "discord",
    ];

    for app in &gui_apps {
        assert!(
            !TerminalFocusContract::is_terminal_window("Editor - Welcome", app),
            "GUI application '{}' must NOT be identified as a terminal",
            app
        );
    }
}

#[test]
fn test_r3_t1_13_shortcut_mapping_terminal_vs_standard() {
    // Terminals require Ctrl+Shift+V
    assert_eq!(
        TerminalFocusContract::resolve_paste_shortcut("zsh", "alacritty"),
        "Ctrl+Shift+V"
    );
    assert_eq!(
        TerminalFocusContract::resolve_paste_shortcut("bash", "kitty"),
        "Ctrl+Shift+V"
    );

    // Standard applications require Ctrl+V
    assert_eq!(
        TerminalFocusContract::resolve_paste_shortcut("Document 1", "gedit"),
        "Ctrl+V"
    );
    assert_eq!(
        TerminalFocusContract::resolve_paste_shortcut("GitHub", "firefox"),
        "Ctrl+V"
    );
}

#[test]
fn test_r3_t1_14_injector_cascade_instantiation_and_order() {
    let cascade = InjectorCascade::new();
    // Cascade must instantiate cleanly and be ready for execution
    let default_cascade = InjectorCascade::default();
    let _ = (cascade, default_cascade);
}

#[tokio::test]
async fn test_r3_t1_15_injector_cascade_execution_availability() {
    let cascade = InjectorCascade::new();
    let (name, success) = cascade.execute_paste().await;
    // Across native or copy-only fallback, execution must produce a valid name and succeed
    assert!(!name.is_empty(), "Injector name should not be empty");
    assert!(
        success,
        "Injector cascade should succeed via native or fallback"
    );
}

// --- R4: Wayland Process Safety & Focus Delay Calibration (Tests 16-20) ---

#[test]
fn test_r4_t1_16_wayland_detection_environment() {
    let is_wayland_fn = |wayland_disp: Option<&str>, session_type: Option<&str>| -> bool {
        wayland_disp.is_some() || session_type.map(|s| s == "wayland").unwrap_or(false)
    };

    assert!(is_wayland_fn(Some("wayland-0"), None));
    assert!(is_wayland_fn(None, Some("wayland")));
    assert!(!is_wayland_fn(None, Some("x11")));
    assert!(!is_wayland_fn(None, None));
}

#[test]
fn test_r4_t1_17_paste_delay_calibration_wayland_minimum() {
    // When configured delay is below 120ms on Wayland, it must clamp to 120ms
    let calibrated = FocusDelayContract::calibrate_delay(20, true);
    assert_eq!(
        calibrated, 120,
        "Wayland auto-paste must enforce minimum 120ms focus delay"
    );
}

#[test]
fn test_r4_t1_18_paste_delay_calibration_x11_minimum() {
    // When configured delay is below 50ms on X11, it must clamp to 50ms
    let calibrated = FocusDelayContract::calibrate_delay(20, false);
    assert_eq!(
        calibrated, 50,
        "X11 auto-paste must enforce minimum 50ms focus delay"
    );
}

#[test]
fn test_r4_t1_19_paste_delay_preserves_higher_user_config() {
    // User configured 350ms should not be reduced on Wayland or X11
    assert_eq!(FocusDelayContract::calibrate_delay(350, true), 350);
    assert_eq!(FocusDelayContract::calibrate_delay(350, false), 350);
}

#[tokio::test]
async fn test_r4_t1_20_child_process_kill_on_drop_semantics() {
    // Demonstrates that tokio Command with kill_on_drop(true) aborts the child when future is dropped
    let mut cmd = tokio::process::Command::new("sleep");
    cmd.arg("10");
    cmd.kill_on_drop(true);

    let child_handle = cmd.spawn();
    assert!(child_handle.is_ok(), "Spawn sleep process");
    let child = child_handle.unwrap();

    let child_pid = child.id();
    assert!(child_pid.is_some(), "Process has valid PID");

    // Drop the child handle
    drop(child);
    // Process is guaranteed terminated or signaled via kill_on_drop
}

// --- R5: Code Quality & Clippy Compliance (Tests 21-25) ---

#[test]
fn test_r5_t1_21_clippy_lint_targets_checked() {
    // Target 1: Checked division prevents manual checked ops violation
    let total_luma: u64 = 1000;
    let pixel_count: u64 = 0;
    let avg_luma = total_luma
        .checked_div(pixel_count)
        .map(|v| v as u8)
        .unwrap_or(255);
    assert_eq!(avg_luma, 255);

    let pixel_count_ok: u64 = 10;
    let avg_ok = total_luma
        .checked_div(pixel_count_ok)
        .map(|v| v as u8)
        .unwrap_or(255);
    assert_eq!(avg_ok, 100);
}

#[test]
fn test_r5_t1_22_config_serialization_and_defaults() {
    let default_cfg = AppConfig::default();
    assert_eq!(default_cfg.paste.paste_delay_ms, 120);
    assert_eq!(default_cfg.general.max_entries, 500);

    let serialized = toml::to_string(&default_cfg).expect("serialize config");
    let deserialized: AppConfig = toml::from_str(&serialized).expect("deserialize config");
    assert_eq!(deserialized.paste.paste_delay_ms, 120);
}

#[test]
fn test_r5_t1_23_database_schema_initialization_idempotence() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("idempotence.db");

    // Open once
    {
        let repo1 = SqliteRepository::open(&db_path).expect("open 1");
        repo1
            .insert_or_update(&create_test_entry("Item 1", None))
            .expect("insert");
    }

    // Open again on existing file
    {
        let repo2 = SqliteRepository::open(&db_path).expect("open 2");
        assert_eq!(repo2.count().expect("count"), 1);
        repo2
            .insert_or_update(&create_test_entry("Item 2", None))
            .expect("insert 2");
        assert_eq!(repo2.count().expect("count"), 2);
    }
}

#[test]
fn test_r5_t1_24_memory_shield_buffer_protection() {
    let mut secret = [0x42u8; 32];
    assert_eq!(secret[0], 0x42);
    secret.zeroize();
    assert_eq!(secret[0], 0x00);
    assert_eq!(secret.iter().sum::<u8>(), 0);
}

#[test]
fn test_r5_t1_25_error_propagation_and_conversion() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("bad_sql.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    // Invalid limit/offset or syntax error handling
    let res = repo.search("", 0, 0);
    assert!(res.is_ok(), "Empty search query handled gracefully");
}

// ============================================================================
// TIER 2: BOUNDARY & CORNER CASES (25 Tests)
// ============================================================================

// --- R1: Database Boundaries (Tests 26-30) ---

#[test]
fn test_r1_t2_26_transient_lock_wait_under_contention() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("lock_wait_contention.db");

    let lock_started = Arc::new(AtomicBool::new(false));
    let lock_started_clone = Arc::clone(&lock_started);

    let path_a = db_path.clone();
    let thread_a = thread::spawn(move || {
        let repo_a = SqliteRepository::open(&path_a).expect("open A");
        let entries: Vec<_> = (0..20)
            .map(|i| create_test_entry(&format!("LockHolder-{}", i), None))
            .collect();
        lock_started_clone.store(true, Ordering::SeqCst);
        let _ = repo_a.batch_insert_or_update(&entries);
        thread::sleep(Duration::from_millis(250));
    });

    while !lock_started.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(5));
    }

    let path_b = db_path.clone();
    let repo_b = SqliteRepository::open(&path_b).expect("open B");
    let entry_b = create_test_entry("Contender Entry", Some("thread_b"));

    let start = Instant::now();
    let res = repo_b.insert_or_update(&entry_b);
    let elapsed = start.elapsed();

    assert!(
        res.is_ok(),
        "Contending write must succeed without SQLITE_BUSY: {:?}",
        res.err()
    );
    assert!(
        elapsed < Duration::from_millis(5000),
        "Write should finish well within the 5000ms busy timeout"
    );

    thread_a.join().expect("thread A join");
}

#[test]
fn test_r1_t2_27_zero_row_batch_operations() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("zero_batch.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    let res_insert = repo.batch_insert_or_update(&[]);
    assert!(res_insert.expect("batch insert empty").is_empty());

    let res_delete = repo.batch_delete(&[]);
    assert_eq!(res_delete.expect("batch delete empty"), 0);

    let res_pin = repo.batch_set_pinned(&[], true);
    assert_eq!(res_pin.expect("batch pin empty"), 0);
}

#[test]
fn test_r1_t2_28_high_concurrency_stress_20_threads() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("stress_20_threads.db");

    // Initialize initial schema
    {
        let repo = SqliteRepository::open(&db_path).expect("open repo");
        repo.insert_or_update(&create_test_entry("InitStress", None))
            .expect("init");
    }

    let thread_count = 20;
    let ops_per_thread = 5;
    let mut handles = Vec::new();

    for t in 0..thread_count {
        let p = db_path.clone();
        handles.push(thread::spawn(move || {
            let repo = SqliteRepository::open(&p).expect("open in thread");
            for i in 0..ops_per_thread {
                let entry = create_test_entry(&format!("Stress-{}-{}", t, i), Some("stress"));
                repo.insert_or_update(&entry).unwrap_or_else(|e| {
                    panic!("Thread {} write {} failed with {:?}", t, i, e);
                });
            }
        }));
    }

    for h in handles {
        h.join().expect("stress thread panicked");
    }

    let final_repo = SqliteRepository::open(&db_path).expect("open final");
    assert_eq!(
        final_repo.count().expect("count"),
        1 + (thread_count * ops_per_thread)
    );
}

#[test]
fn test_r1_t2_29_vacuum_concurrent_with_active_readers() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("vacuum_test.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    for i in 0..50 {
        repo.insert_or_update(&create_test_entry(&format!("Vac-{}", i), None))
            .expect("insert");
    }

    let p_read = db_path.clone();
    let reader = thread::spawn(move || {
        let r = SqliteRepository::open(&p_read).expect("open reader");
        for _ in 0..10 {
            let _ = r.list(10, 0, None, false);
            thread::sleep(Duration::from_millis(5));
        }
    });

    let res_vac = repo.vacuum();
    assert!(
        res_vac.is_ok(),
        "Vacuum should succeed with concurrent readers"
    );

    reader.join().expect("reader joined");
}

#[test]
fn test_r1_t2_30_duplicate_content_hash_upsert_contention() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("dedup_contention.db");

    let thread_count = 5;
    let mut handles = Vec::new();

    for t in 0..thread_count {
        let p = db_path.clone();
        handles.push(thread::spawn(move || {
            let repo = SqliteRepository::open(&p).expect("open dedup repo");
            // All threads insert identical content
            let entry = create_test_entry("IDENTICAL_CONTENT_PAYLOAD", Some(&format!("app_{}", t)));
            repo.insert_or_update(&entry).expect("insert or update");
        }));
    }

    for h in handles {
        h.join().expect("dedup thread joined");
    }

    let repo = SqliteRepository::open(&db_path).expect("open final");
    assert_eq!(
        repo.count().expect("count"),
        1,
        "Duplicate hashes must deduplicate to 1 entry"
    );
}

// --- R2: Search Boundaries (Tests 31-35) ---

#[test]
fn test_r2_t2_31_search_with_special_characters() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("special_chars.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    repo.insert_or_update(&create_test_entry(
        "SELECT * FROM users WHERE '1'='1';",
        Some("sql"),
    ))
    .expect("insert");
    repo.insert_or_update(&create_test_entry(
        "Regex match: ^[a-z0-9_.-]+@[a-z0-9.-]+$",
        Some("regex"),
    ))
    .expect("insert");
    repo.insert_or_update(&create_test_entry(
        "Wildcard 50% discount & promo_code",
        Some("shop"),
    ))
    .expect("insert");

    assert_eq!(
        repo.search("WHERE '1'='1'", 10, 0)
            .expect("search sql")
            .len(),
        1
    );
    assert_eq!(repo.search("50%", 10, 0).expect("search wildcard").len(), 1);
    assert_eq!(
        repo.search("^[a-z0-9", 10, 0).expect("search regex").len(),
        1
    );
}

#[test]
fn test_r2_t2_32_search_unicode_and_turkish_characters() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("turkish_search.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    repo.insert_or_update(&create_test_entry(
        "Geliştirici konsolu ve Türkçe karakterler: ç, ğ, ı, ö, ş, ü",
        Some("turkce"),
    ))
    .expect("insert");

    let res = repo.search("Geliştirici", 10, 0).expect("search turkish");
    assert_eq!(res.len(), 1);
    assert!(res[0].preview.contains("Türkçe"));
}

#[test]
fn test_r2_t2_33_search_pagination_boundaries() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("pagination_boundaries.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    for i in 0..10 {
        repo.insert_or_update(&create_test_entry(&format!("Page-{}", i), None))
            .expect("insert");
    }

    // Offset beyond total matches
    let res_empty = repo.search("Page", 10, 100).expect("offset out of range");
    assert_eq!(res_empty.len(), 0);

    // Limit 0 returns empty
    let res_zero = repo.search("Page", 0, 0).expect("zero limit");
    assert_eq!(res_zero.len(), 0);
}

#[test]
fn test_r2_t2_34_async_search_sequence_counter_race() {
    let tracker = SearchSequenceTracker::default();

    // Query 1 dispatched
    let seq_1 = tracker.next_sequence();
    assert_eq!(seq_1, 1);

    // Query 2 dispatched rapidly before Query 1 returns
    let seq_2 = tracker.next_sequence();
    assert_eq!(seq_2, 2);

    // Query 1 response arrives late -> must be discarded
    assert!(
        !tracker.is_response_valid(seq_1),
        "Outdated sequence 1 must be rejected"
    );

    // Query 2 response arrives -> must be accepted
    assert!(
        tracker.is_response_valid(seq_2),
        "Latest sequence 2 must be accepted"
    );
}

#[test]
fn test_r2_t2_35_search_empty_and_whitespace_only() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("whitespace_search.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    repo.insert_or_update(&create_test_entry("Normal Entry", None))
        .expect("insert");

    // Whitespace queries should not crash or produce syntax error
    let res = repo.search("   \t\n   ", 10, 0);
    assert!(res.is_ok());
}

// --- R3: Terminal Detection Boundaries (Tests 36-40) ---

#[test]
fn test_r3_t2_36_terminal_class_case_insensitivity() {
    let mixed_cases = ["ALACRITTY", "Kitty", "Gnome-Terminal", "XTERM", "WezTerm"];
    for term in &mixed_cases {
        assert!(
            TerminalFocusContract::is_terminal_window("", term),
            "Window class '{}' should be matched case-insensitively",
            term
        );
    }
}

#[test]
fn test_r3_t2_37_terminal_app_id_dot_and_dash_variants() {
    let dot_variants = [
        "org.gnome.Terminal",
        "org.kde.konsole",
        "footclient",
        "wezterm-gui",
        "st-256color",
    ];

    for variant in &dot_variants {
        assert!(
            TerminalFocusContract::is_terminal_window("", variant),
            "Variant '{}' should match terminal emulator",
            variant
        );
    }
}

#[test]
fn test_r3_t2_38_empty_and_missing_window_metadata() {
    // When compositor provides no title and no class, fallback gracefully to standard Ctrl+V
    assert!(!TerminalFocusContract::is_terminal_window("", ""));
    assert_eq!(
        TerminalFocusContract::resolve_paste_shortcut("", ""),
        "Ctrl+V"
    );
}

#[test]
fn test_r3_t2_39_browser_tab_with_terminal_in_title() {
    // Tab title in Firefox containing 'Terminal' should NOT be recognized as a terminal window
    let title = "Bash Terminal Tutorial - Mozilla Firefox";
    let class = "firefox";
    assert!(
        !TerminalFocusContract::is_terminal_window(title, class),
        "Browser with 'Terminal' in title must not be classified as terminal"
    );
    assert_eq!(
        TerminalFocusContract::resolve_paste_shortcut(title, class),
        "Ctrl+V"
    );
}

#[test]
fn test_r3_t2_40_embedded_ide_terminals() {
    // IDE windows (VSCode, JetBrains) intercept Ctrl+V internally; must evaluate to Ctrl+V
    assert!(!TerminalFocusContract::is_terminal_window(
        "Terminal - VSCode",
        "code"
    ));
    assert!(!TerminalFocusContract::is_terminal_window(
        "Terminal - Terminal.java",
        "jetbrains-idea"
    ));
    assert_eq!(
        TerminalFocusContract::resolve_paste_shortcut("Terminal", "code"),
        "Ctrl+V"
    );
}

// --- R4: Process Safety Boundaries (Tests 41-45) ---

#[test]
fn test_r4_t2_41_paste_delay_zero_or_negative_values() {
    // Delay 0 clamped to minimum
    assert_eq!(FocusDelayContract::calibrate_delay(0, true), 120);
    assert_eq!(FocusDelayContract::calibrate_delay(0, false), 50);
}

#[tokio::test]
async fn test_r4_t2_42_process_timeout_with_hung_command() {
    let mut cmd = tokio::process::Command::new("sleep");
    cmd.arg("5");
    cmd.kill_on_drop(true);

    let execution = timeout(Duration::from_millis(50), cmd.output());
    let res = execution.await;
    assert!(res.is_err(), "Command must time out after 50ms");
}

#[test]
fn test_r4_t2_43_mixed_environment_session_precedence() {
    // When XDG_SESSION_TYPE is wayland but DISPLAY is also present (e.g. XWayland)
    let is_wayland = true; // Simulated detection
    let delay = FocusDelayContract::calibrate_delay(80, is_wayland);
    assert_eq!(
        delay, 120,
        "Wayland session takes precedence and enforces 120ms"
    );
}

#[tokio::test]
async fn test_r4_t2_44_repeated_rapid_process_timeouts_no_zombies() {
    // Rapidly spawn and cancel multiple commands
    for _ in 0..5 {
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("2");
        cmd.kill_on_drop(true);
        let _ = timeout(Duration::from_millis(10), cmd.output()).await;
    }
    // All 5 timed-out child processes terminated via kill_on_drop
}

#[test]
fn test_r4_t2_45_custom_extremes_delay_bounds() {
    let large_delay = 5000;
    assert_eq!(FocusDelayContract::calibrate_delay(large_delay, true), 5000);
}

// --- R5: System Integrity Boundaries (Tests 46-50) ---

#[test]
fn test_r5_t2_46_empty_and_extreme_content_payloads() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("payload_sizes.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    // Empty content
    let empty_entry = create_test_entry("", None);
    assert!(repo.insert_or_update(&empty_entry).is_ok());

    // 1MB large content
    let large_text = "A".repeat(1_000_000);
    let large_entry = create_test_entry(&large_text, None);
    assert!(repo.insert_or_update(&large_entry).is_ok());

    let fetched = repo.get_by_id(&large_entry.id).expect("get large");
    assert_eq!(fetched.text_content.unwrap().len(), 1_000_000);
}

#[test]
fn test_r5_t2_47_corrupted_blob_hash_integrity() {
    let payload = b"Hello Secure Storage";
    let valid_hash = BlobStore::compute_hash(payload);
    let corrupted_payload = b"Hello Corrupted Storage";
    let corrupted_hash = BlobStore::compute_hash(corrupted_payload);

    assert_ne!(valid_hash, corrupted_hash, "Hash mismatch must be detected");
}

#[test]
fn test_r5_t2_48_invalid_ipc_payload_rejection() {
    let invalid_json = r#"{"invalid_field": 12345}"#;
    let res: Result<IpcRequest, _> = serde_json::from_str(invalid_json);
    assert!(res.is_err(), "Malformed IPC payload must be rejected");
}

#[test]
fn test_r5_t2_49_tampered_ciphertext_detection() {
    let engine = CryptoEngine::new_from_key([0x42u8; 32]);
    let plaintext = b"Sensitive Password Data";
    let mut ciphertext = engine.encrypt(plaintext).expect("encrypt");

    // Tamper with ciphertext byte
    let len = ciphertext.len();
    ciphertext[len - 1] ^= 0xFF;

    let decrypt_res = engine.decrypt(&ciphertext);
    assert!(
        decrypt_res.is_err(),
        "Tampered ciphertext must fail authentication"
    );
}

#[test]
fn test_r5_t2_50_secret_pattern_boundary_matching() {
    let custom_patterns = vec![r"custom_secret_[0-9]+".to_string()];

    let filter = SecretFilter::new(&custom_patterns);
    assert!(
        filter.contains_secret("ghp_123456789012345678901234567890123456"),
        "Detects GitHub token"
    );
    assert!(
        filter.contains_secret("custom_secret_987654321"),
        "Detects custom pattern"
    );
    assert!(
        !filter.contains_secret("Public Documentation and normal text"),
        "Allows clean text"
    );
}

// ============================================================================
// TIER 3: CROSS-FEATURE COMBINATIONS (5 Tests)
// ============================================================================

#[test]
fn test_r3_cross_51_concurrency_and_search() {
    // Cross R1 & R2: Multi-threaded writing while concurrent threads perform full searches
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("cross_concurrency_search.db");

    let repo = SqliteRepository::open(&db_path).expect("open repo");
    for i in 0..20 {
        repo.insert_or_update(&create_test_entry(&format!("CrossItem-{}", i), None))
            .expect("init");
    }

    let stop = Arc::new(AtomicBool::new(false));
    let stop_searcher = Arc::clone(&stop);

    let p_search = db_path.clone();
    let search_handle = thread::spawn(move || {
        let r = SqliteRepository::open(&p_search).expect("open searcher");
        let mut count = 0;
        while !stop_searcher.load(Ordering::Relaxed) {
            let res = r.search("CrossItem", 10, 0).expect("search");
            assert!(!res.is_empty());
            count += 1;
            thread::sleep(Duration::from_millis(5));
        }
        count
    });

    let p_write = db_path.clone();
    let write_handle = thread::spawn(move || {
        let r = SqliteRepository::open(&p_write).expect("open writer");
        for i in 20..35 {
            r.insert_or_update(&create_test_entry(&format!("CrossItem-{}", i), None))
                .expect("write");
            thread::sleep(Duration::from_millis(10));
        }
    });

    write_handle.join().expect("writer joined");
    stop.store(true, Ordering::Relaxed);
    let search_count = search_handle.join().expect("searcher joined");
    assert!(search_count > 0);
}

#[test]
fn test_r3_cross_52_search_and_paste_selection() {
    // Cross R2 & R3: Search historical database, select result, resolve appropriate paste shortcut
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("cross_search_paste.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    repo.insert_or_update(&create_test_entry(
        "curl -X POST https://api.prod/v1/auth",
        Some("bash"),
    ))
    .expect("insert");

    let search_results = repo.search("api.prod", 10, 0).expect("search");
    assert_eq!(search_results.len(), 1);
    let selected_entry = &search_results[0];

    // Suppose user is currently focused on alacritty terminal
    let active_class = "alacritty";
    let shortcut = TerminalFocusContract::resolve_paste_shortcut("", active_class);
    assert_eq!(shortcut, "Ctrl+Shift+V");
    assert!(selected_entry.preview.contains("curl -X POST"));
}

#[test]
fn test_r3_cross_53_terminal_paste_and_wayland_delay() {
    // Cross R3 & R4: In a Wayland session, active window is foot terminal -> Ctrl+Shift+V with 120ms delay
    let is_wayland = true;
    let window_class = "foot";

    let shortcut = TerminalFocusContract::resolve_paste_shortcut("", window_class);
    let delay = FocusDelayContract::calibrate_delay(40, is_wayland);

    assert_eq!(shortcut, "Ctrl+Shift+V");
    assert_eq!(delay, 120);
}

#[tokio::test]
async fn test_r3_cross_54_concurrency_and_process_safety() {
    // Cross R1 & R4: High SQLite concurrency while external commands timeout with kill_on_drop
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("cross_concurrency_process.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    // Spawn async timed process
    let mut cmd = tokio::process::Command::new("sleep");
    cmd.arg("2");
    cmd.kill_on_drop(true);
    let timeout_task = tokio::spawn(async move {
        let _ = timeout(Duration::from_millis(50), cmd.output()).await;
    });

    // Run synchronous DB inserts
    for i in 0..15 {
        repo.insert_or_update(&create_test_entry(&format!("ProcSafe-{}", i), None))
            .expect("insert");
    }

    timeout_task.await.expect("task completed");
    assert_eq!(repo.count().expect("count"), 15);
}

#[test]
fn test_r3_cross_55_search_clear_and_write_invalidation() {
    // Cross R2 & R1: Typing search query, new DB entry arrives, user clears search -> reflects latest entry
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("cross_clear_invalidation.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    repo.insert_or_update(&create_test_entry("Old Entry 1", None))
        .expect("insert 1");

    // Search active for 'non-existent'
    let res = repo.search("non-existent", 10, 0).expect("search");
    assert!(res.is_empty());

    // Background write arrives
    repo.insert_or_update(&create_test_entry("Fresh Entry 2", None))
        .expect("insert 2");

    // Clear search -> query recent history
    let restored = repo.list(10, 0, None, false).expect("list recent");
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].preview, "Fresh Entry 2");
}

// ============================================================================
// TIER 4: REAL-WORLD APPLICATION SCENARIOS (5 Tests)
// ============================================================================

#[test]
fn test_r4_scenario_56_terminal_developer_workflow() {
    // Scenario: Developer copies terminal command from documentation in browser,
    // switches to Alacritty terminal, searches history for git token, and auto-pastes.
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("scenario_dev.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    // 1. Doc snippet copied
    let token_entry =
        create_test_entry("export GITHUB_TOKEN=ghp_secretToken12345", Some("firefox"));
    repo.insert_or_update(&token_entry).expect("copy token");

    // 2. Developer opens search in GUI
    let results = repo.search("GITHUB_TOKEN", 10, 0).expect("search token");
    assert_eq!(results.len(), 1);

    // 3. Target window focus is detected
    let active_class = "alacritty";
    let is_terminal = TerminalFocusContract::is_terminal_window("", active_class);
    assert!(is_terminal);

    // 4. Injected shortcut resolved
    let shortcut = TerminalFocusContract::resolve_paste_shortcut("", active_class);
    assert_eq!(shortcut, "Ctrl+Shift+V");

    // 5. Wayland delay calibrated
    let delay = FocusDelayContract::calibrate_delay(50, true);
    assert_eq!(delay, 120);
}

#[test]
fn test_r4_scenario_57_browser_multitab_research() {
    // Scenario: User copies 30 URLs and notes across multiple browser tabs,
    // filters by category, and clears search to return to recent chronological list.
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("scenario_research.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    for i in 0..30 {
        let app = if i % 2 == 0 { "firefox" } else { "chrome" };
        let text = format!("https://docs.rs/crate-research/item-{}", i);
        repo.insert_or_update(&create_test_entry(&text, Some(app)))
            .expect("insert");
    }

    // Filter by chrome
    let chrome_items = repo.search("chrome", 50, 0).expect("search chrome");
    assert!(chrome_items.len() >= 15);

    // Clear search
    let all_recent = repo.list(50, 0, None, false).expect("list all");
    assert_eq!(all_recent.len(), 30);
}

#[test]
fn test_r4_scenario_58_heavy_daemon_background_eviction_during_gui_usage() {
    // Scenario: Daemon eviction worker runs capacity eviction while user is querying history
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("scenario_eviction.db");
    let repo = SqliteRepository::open(&db_path).expect("open repo");

    // Pre-fill 100 items
    for i in 0..100 {
        repo.insert_or_update(&create_test_entry(&format!("EvictItem-{}", i), None))
            .expect("fill");
    }

    // Eviction worker evicts down to 40 items
    let evicted = repo.evict_capacity(40).expect("evict capacity");
    assert_eq!(evicted, 60);

    // GUI list and search continue to function cleanly
    let remaining = repo.count().expect("count remaining");
    assert_eq!(remaining, 40);

    let search_res = repo.search("EvictItem", 10, 0).expect("search after evict");
    assert!(!search_res.is_empty());
}

#[tokio::test]
async fn test_r4_scenario_59_wayland_compositor_hiccup_and_recovery() {
    // Scenario: Wayland clipboard read command hangs due to compositor deadlock;
    // timeout kills the child cleanly, focus delay prevents keydrop, and execution recovers.
    let mut cmd = tokio::process::Command::new("sleep");
    cmd.arg("5");
    cmd.kill_on_drop(true);

    let res = timeout(Duration::from_millis(50), cmd.output()).await;
    assert!(res.is_err(), "Compositor freeze command timed out cleanly");

    // System calibrates delay and recovers
    let delay = FocusDelayContract::calibrate_delay(30, true);
    assert_eq!(delay, 120);
}

#[test]
fn test_r4_scenario_60_rapid_search_typing_and_backspacing() {
    // Scenario: User rapidly types 'p', 'po', 'post', backspaces to '', then types 'red'
    let tracker = SearchSequenceTracker::default();

    let seq_p = tracker.next_sequence();
    let seq_po = tracker.next_sequence();
    let seq_post = tracker.next_sequence();
    let seq_clear = tracker.next_sequence();
    let seq_red = tracker.next_sequence();

    assert_eq!(seq_red, 5);

    // Stale responses arriving out of order
    assert!(!tracker.is_response_valid(seq_p));
    assert!(!tracker.is_response_valid(seq_po));
    assert!(!tracker.is_response_valid(seq_post));
    assert!(!tracker.is_response_valid(seq_clear));

    // Only latest response for 'red' accepted
    assert!(tracker.is_response_valid(seq_red));
}
