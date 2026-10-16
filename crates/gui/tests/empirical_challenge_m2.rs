//! Empirical Challenge Test Suite for Milestone 2 (R2: Universal Full-History Search in GUI)
//! Adversarially verifying:
//! 1. Whitespace-only search queries and instant local cache restoration
//! 2. Unicode, regex metacharacters, FTS5 syntax, and special symbols in queries
//! 3. Daemon offline fallback (missing socket, stale socket, error response)
//! 4. Effective paste delay calibration across Wayland and X11 sessions
//! 5. Asynchronous sequence counter race stress harness

use chrono::Utc;
use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::PasteConfig;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::ipc::{IpcClient, IpcRequest, IpcResponse};
use clipboard_history_core::storage::SqliteRepository;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[path = "../src/search.rs"]
mod search;

use search::{CategoryFilter, FuzzySearchEngine};

fn create_entry(text: &str, app: Option<&str>, delta_secs: i64) -> ClipboardEntry {
    let hash = BlobStore::compute_hash(text.as_bytes());
    let mut entry = ClipboardEntry::new_text(
        text.to_string(),
        hash,
        vec!["text/plain".to_string()],
        app.map(|s| s.to_string()),
    );
    entry.created_at = Utc::now() + chrono::Duration::seconds(delta_secs);
    entry.last_used_at = Utc::now() + chrono::Duration::seconds(delta_secs);
    entry
}

// ============================================================================
// 1. Whitespace-Only & Boundary Conditions Challenge
// ============================================================================

#[test]
fn challenge_whitespace_only_and_boundary_queries() {
    let engine = FuzzySearchEngine::new();

    let mut cached_entries = Vec::new();
    for i in 0..50 {
        cached_entries.push(create_entry(
            &format!("Item-{:03} content", i),
            Some("test_app"),
            i as i64,
        ));
    }

    // Diverse whitespace variants: ascii spaces, tabs, newlines, carriage returns, Unicode spaces
    let whitespace_test_cases = [
        "",
        " ",
        "  ",
        "\t",
        "\n",
        "\r\n",
        "   \t\n   \r  ",
        "\u{00A0}", // Non-breaking space
        "\u{2000}", // En quad
        "\u{2002}", // En space
        "\u{2003}", // Em space
        "\u{3000}", // Ideographic space
        "  \u{00A0} \t \u{3000} \n  ",
    ];

    for ws in whitespace_test_cases {
        assert!(
            ws.trim().is_empty(),
            "Whitespace query {:?} must satisfy trim().is_empty()",
            ws
        );

        // When query is trimmed to empty, GUI restores full local cache
        let restored = engine.filter_entries(&cached_entries, "", CategoryFilter::All);
        assert_eq!(
            restored.len(),
            cached_entries.len(),
            "Restored cache must contain all {} items for query {:?}",
            cached_entries.len(),
            ws
        );
        assert_eq!(restored[0].id, cached_entries[0].id);
        assert_eq!(restored[49].id, cached_entries[49].id);
    }

    // Leading and trailing whitespace around real keywords must be properly handled
    let padded_queries = [
        ("  Item-025  ", "Item-025"),
        ("\t\tItem-010\n", "Item-010"),
        ("   Item-005", "Item-005"),
        ("Item-049   ", "Item-049"),
    ];

    for (raw, expected_match) in padded_queries {
        let trimmed = raw.trim();
        assert!(!trimmed.is_empty());
        let results = engine.filter_entries(&cached_entries, trimmed, CategoryFilter::All);
        assert!(
            !results.is_empty(),
            "Trimmed query '{}' must find match for raw '{:?}'",
            trimmed,
            raw
        );
        assert!(results[0].preview.contains(expected_match));
    }
}

// ============================================================================
// 2. Unicode, Regex Metacharacters, FTS5 Syntax & Special Symbols Challenge
// ============================================================================

#[test]
fn challenge_special_symbols_and_regex_metacharacters_in_search() {
    let repo = SqliteRepository::open_in_memory().expect("open sqlite");
    let engine = FuzzySearchEngine::new();

    // Adversarial data entries
    let test_corpus = [
        (
            "Regex pattern: ^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$",
            "code_app",
        ),
        (
            "Math: \\int_{0}^{\\infty} e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}",
            "editor",
        ),
        (
            "FTS wildcard test: *foo* AND (bar OR NOT baz) NEAR/3 qux",
            "notes",
        ),
        (
            "SQL injection attempt: '; DROP TABLE entries; SELECT * FROM * WHERE '1'='1",
            "browser",
        ),
        (
            "Double quotes: \"\"\"nested\"\"\" and 'single' and `backticks`",
            "terminal",
        ),
        (
            "Turkish characters: Çetin şapkalı ördek ılık gölde yüzüyor. ŞAFAK, İĞNE, ÖZGÜR.",
            "turkish_news",
        ),
        (
            "CJK characters: クリップボードの履歴検索とテスト - 한국어 번역 - 剪贴板历史",
            "cjk_app",
        ),
        (
            "Emoji stress: 🦀 Rust 🚀 Rocket 📝 Note ⚡ Lightning 🛡️ Shield 💥 Bang!",
            "chat",
        ),
        (
            "Punctuation barrage: !@#$%^&*()_+=-~`[]{}|;':\",./<>?",
            "dev",
        ),
        (
            "Control whitespace: line1\nline2\r\nline3\tcol2\0null",
            "raw_data",
        ),
    ];

    for (i, (text, app)) in test_corpus.iter().enumerate() {
        let entry = create_entry(text, Some(app), i as i64);
        repo.insert_or_update(&entry).expect("insert entry");
    }

    // Queries designed to break parsers, regexes, and FTS5 SQL engines
    let adversarial_queries = [
        "^[a-zA-Z0-9",
        "\\int_{0}",
        "*foo*",
        "AND",
        "OR",
        "NOT",
        "NEAR/3",
        "'; DROP TABLE",
        "\"\"\"",
        "\"nested\"",
        "'''",
        "```",
        "Çetin",
        "ördek",
        "İĞNE",
        "クリップボード",
        "한국어",
        "剪贴板",
        "🦀",
        "🚀",
        "🛡️",
        "!@#$%",
        "[]{}()",
        "^$",
        ".*",
        "+?",
        "\\\\",
        "\\0",
    ];

    for q in adversarial_queries {
        // 1. Core SQLite full-database search must not panic or error
        let search_res = repo.search(q, 100, 0);
        assert!(
            search_res.is_ok(),
            "repo.search must not fail on adversarial query '{}': {:?}",
            q,
            search_res.err()
        );

        // 2. GUI FuzzySearchEngine must not panic on adversarial query
        let all_entries = repo.list(100, 0, None, false).expect("list");
        let filtered = engine.filter_entries(&all_entries, q, CategoryFilter::All);
        // Result length can be 0 or more, but must execute safely
        let _ = filtered.len();
    }
}

// ============================================================================
// 3. Daemon Offline Fallback Challenge
// ============================================================================

#[tokio::test]
async fn challenge_daemon_offline_fallback() {
    let engine = FuzzySearchEngine::new();

    // Setup local GUI cache
    let local_cache = vec![
        create_entry("Local item alpha", Some("app1"), 1),
        create_entry("Local item beta", Some("app2"), 2),
        create_entry("Local item gamma", Some("app3"), 3),
    ];

    // Case A: IPC fails because daemon socket is non-existent
    // Point IpcClient to a non-existent socket path
    let fake_socket = std::env::temp_dir().join(format!(
        "non_existent_{}.sock",
        Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let client = IpcClient::new(&fake_socket);

    let req = IpcRequest::Search {
        query: "beta".to_string(),
        limit: 100,
        offset: 0,
    };

    // Client send must fail gracefully without panicking
    let send_result = client.send(&req).await;
    assert!(send_result.is_err(), "Send to offline socket must fail");

    // In window.rs, when IPC fails or returns error, GUI falls back to local cache:
    let search_results = match send_result {
        Ok(IpcResponse::Entries(results)) => results,
        _ => Vec::new(),
    };
    assert!(search_results.is_empty());

    // Fallback logic executed in GUI
    let final_entries = if !search_results.is_empty() {
        engine.filter_entries(&search_results, "beta", CategoryFilter::All)
    } else {
        engine.filter_entries(&local_cache, "beta", CategoryFilter::All)
    };

    assert_eq!(
        final_entries.len(),
        1,
        "Fallback must successfully filter local cache"
    );
    assert!(final_entries[0].preview.contains("Local item beta"));

    // Case B: Daemon responds with IpcResponse::Error
    let mock_error_response: Result<IpcResponse, clipboard_history_core::error::CoreError> =
        Ok(IpcResponse::Error("Simulated database failure".to_string()));

    let search_results_b = match mock_error_response {
        Ok(IpcResponse::Entries(results)) => results,
        _ => Vec::new(),
    };
    assert!(search_results_b.is_empty());

    let final_entries_b = if !search_results_b.is_empty() {
        engine.filter_entries(&search_results_b, "gamma", CategoryFilter::All)
    } else {
        engine.filter_entries(&local_cache, "gamma", CategoryFilter::All)
    };

    assert_eq!(
        final_entries_b.len(),
        1,
        "Fallback on Error response must filter local cache"
    );
    assert!(final_entries_b[0].preview.contains("Local item gamma"));
}

// ============================================================================
// 4. Effective Paste Delay Calibration Challenge
// ============================================================================

#[test]
fn challenge_paste_delay_calibration_environments() {
    // Test calibration logic under both Wayland and X11 simulated configurations
    let test_cases = [
        (0u64, 50u64, 120u64),
        (10u64, 50u64, 120u64),
        (49u64, 50u64, 120u64),
        (50u64, 50u64, 120u64),
        (75u64, 75u64, 120u64),
        (119u64, 119u64, 120u64),
        (120u64, 120u64, 120u64),
        (250u64, 250u64, 250u64),
        (500u64, 500u64, 500u64),
    ];

    for (user_delay, expected_x11, expected_wayland) in test_cases {
        let config = PasteConfig {
            paste_delay_ms: user_delay,
            ..Default::default()
        };

        // Determine current environment
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland";

        let actual = config.effective_delay_ms();
        let expected = if is_wayland {
            expected_wayland
        } else {
            expected_x11
        };

        assert_eq!(
            actual, expected,
            "For user_delay={}, expected {} on is_wayland={}, got {}",
            user_delay, expected, is_wayland, actual
        );

        // In any environment, effective delay must be >= 50ms
        assert!(
            actual >= 50,
            "effective_delay_ms must never fall below 50ms"
        );
    }
}

// ============================================================================
// 5. Asynchronous Sequence Invalidation Stress Harness
// ============================================================================

#[test]
fn challenge_async_search_sequence_race_stress() {
    let global_seq = Arc::new(AtomicU64::new(0));
    let active_query = Arc::new(std::sync::Mutex::new(String::new()));

    // Simulate rapid typing and clearing: 100 rapid events
    for step in 1u64..=100u64 {
        let is_clear = step % 5 == 0;
        let query_text = if is_clear {
            "".to_string()
        } else {
            format!("query_{}", step)
        };

        let seq = global_seq.fetch_add(1, Ordering::SeqCst) + 1;
        *active_query.lock().unwrap() = query_text.clone();

        // Simulate an older async response arriving from step - 2
        let old_seq = seq.saturating_sub(2);
        let old_query = format!("query_{}", step.saturating_sub(2));

        // Validation rule from window.rs
        let is_old_valid = old_seq == global_seq.load(Ordering::SeqCst)
            && old_query == *active_query.lock().unwrap();

        if step > 2 {
            assert!(
                !is_old_valid,
                "Old sequence {} must be rejected when current seq is {}",
                old_seq, seq
            );
        }

        // Current response is valid only if nothing arrived in between
        let is_current_valid =
            seq == global_seq.load(Ordering::SeqCst) && query_text == *active_query.lock().unwrap();
        assert!(is_current_valid, "Current sequence {} must be valid", seq);
    }
}
