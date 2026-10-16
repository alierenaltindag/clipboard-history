use chrono::Utc;
use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::domain::ClipboardEntry;
use clipboard_history_core::storage::SqliteRepository;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[path = "../src/search.rs"]
mod search;

use search::{CategoryFilter, FuzzySearchEngine};

fn make_test_entry(text: &str, app: Option<&str>, i: i64) -> ClipboardEntry {
    let hash = BlobStore::compute_hash(text.as_bytes());
    let mut entry = ClipboardEntry::new_text(
        text.to_string(),
        hash,
        vec!["text/plain".to_string()],
        app.map(|s| s.to_string()),
    );
    entry.created_at = Utc::now() + chrono::Duration::seconds(i);
    entry.last_used_at = Utc::now() + chrono::Duration::seconds(i);
    entry
}

/// Adversarially stress-tests race conditions with simulated async out-of-order responses.
/// Simulates rapid typing, backspacing, clearing, and late-arriving responses.
#[tokio::test]
async fn test_adversarial_rapid_typing_and_backspacing_race() {
    let search_seq = Rc::new(RefCell::new(0u64));
    let current_query = Rc::new(RefCell::new(String::new()));
    let displayed_results = Rc::new(RefCell::new(Vec::<String>::new()));

    // Cache of 100 items representing local cache
    let local_cache: Vec<String> = (0..100).map(|i| format!("item-{:03}", i)).collect();
    *displayed_results.borrow_mut() = local_cache.clone();

    // Helper simulating GUI search logic from window.rs
    let simulate_search = {
        let seq = Rc::clone(&search_seq);
        let cur_q = Rc::clone(&current_query);
        let disp = Rc::clone(&displayed_results);
        let cache = local_cache.clone();

        move |raw_text: &str| -> Option<(u64, String)> {
            let trimmed = raw_text.trim().to_string();
            *cur_q.borrow_mut() = trimmed.clone();

            if trimmed.is_empty() {
                // Instant zero-lag restoration
                let next = seq.borrow().wrapping_add(1);
                *seq.borrow_mut() = next;
                *disp.borrow_mut() = cache.clone();
                None
            } else {
                let next = seq.borrow().wrapping_add(1);
                *seq.borrow_mut() = next;
                Some((next, trimmed))
            }
        }
    };

    // Helper simulating response arrival from daemon
    let simulate_response = {
        let seq = Rc::clone(&search_seq);
        let cur_q = Rc::clone(&current_query);
        let disp = Rc::clone(&displayed_results);

        move |resp_seq: u64, resp_query: String, data: Vec<String>| -> bool {
            // Guard check from window.rs line 691
            if *seq.borrow() != resp_seq || *cur_q.borrow() != resp_query {
                return false; // Stale or mismatched response discarded!
            }
            *disp.borrow_mut() = data;
            true
        }
    };

    // Keystroke sequence simulating rapid typing and backspacing:
    // 1. User types "q"
    let req1 = simulate_search("q").expect("active query");
    assert_eq!(req1.0, 1);

    // 2. User rapidly types "qu"
    let req2 = simulate_search("qu").expect("active query");
    assert_eq!(req2.0, 2);

    // 3. User rapidly types "query"
    let req3 = simulate_search("query").expect("active query");
    assert_eq!(req3.0, 3);

    // 4. User backspaces all the way to ""
    let empty_res = simulate_search("");
    assert!(empty_res.is_none(), "Empty query produces no async task");
    assert_eq!(*search_seq.borrow(), 4);
    assert_eq!(
        displayed_results.borrow().len(),
        100,
        "Cache restored instantly"
    );

    // 5. Stale responses from req1, req2, req3 arrive late (after clear)
    let accepted1 = simulate_response(req1.0, req1.1, vec!["result_q".to_string()]);
    assert!(!accepted1, "Late req1 must be rejected after clear");
    assert_eq!(displayed_results.borrow().len(), 100);

    let accepted2 = simulate_response(req2.0, req2.1, vec!["result_qu".to_string()]);
    assert!(!accepted2, "Late req2 must be rejected after clear");
    assert_eq!(displayed_results.borrow().len(), 100);

    let accepted3 = simulate_response(req3.0, req3.1, vec!["result_query".to_string()]);
    assert!(!accepted3, "Late req3 must be rejected after clear");
    assert_eq!(displayed_results.borrow().len(), 100);

    // 6. User now types "another"
    let req5 = simulate_search("another").expect("active query");
    assert_eq!(req5.0, 5);

    // 7. Suppose another stale response with matching seq (5) but outdated query arrives
    let accepted_mismatch = simulate_response(5, "old_query".to_string(), vec!["bad".to_string()]);
    assert!(
        !accepted_mismatch,
        "Query text mismatch must reject even if seq matches"
    );

    // 8. Legitimate response for req5 arrives
    let accepted5 = simulate_response(req5.0, req5.1, vec!["result_another".to_string()]);
    assert!(accepted5, "Latest valid response must be accepted");
    assert_eq!(
        *displayed_results.borrow(),
        vec!["result_another".to_string()]
    );
}

/// Stress test with 200 randomized async keystroke and response events.
#[tokio::test]
async fn test_fuzz_random_typing_and_concurrent_in_flight_resolutions() {
    let search_seq = Arc::new(AtomicU64::new(0));
    let cur_query = Arc::new(Mutex::new(String::new()));
    let ui_view = Arc::new(Mutex::new(Vec::<String>::new()));

    let full_cache: Vec<String> = (0..100).map(|i| format!("item-{:03}", i)).collect();
    *ui_view.lock().unwrap() = full_cache.clone();

    let mut rng_seed: u64 = 0xDEADBEEFCAFE;
    let mut next_rand = || {
        rng_seed = rng_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        rng_seed
    };

    let words = vec![
        "", "apple", "banana", "cat", "dog", "elephant", "   ", "rust", "cargo", "",
    ];

    for _ in 0..200 {
        let word_idx = (next_rand() as usize) % words.len();
        let query = words[word_idx];
        let trimmed = query.trim().to_string();

        let (seq, active_task) = {
            *cur_query.lock().unwrap() = trimmed.clone();
            let next_seq = search_seq.fetch_add(1, Ordering::SeqCst) + 1;
            if trimmed.is_empty() {
                *ui_view.lock().unwrap() = full_cache.clone();
                (next_seq, false)
            } else {
                (next_seq, true)
            }
        };

        if active_task {
            // Spawn simulated async response with variable delay
            let s_seq = Arc::clone(&search_seq);
            let s_query = Arc::clone(&cur_query);
            let s_view = Arc::clone(&ui_view);
            let delay_micros = next_rand() % 500;
            let captured_seq = seq;
            let captured_q = trimmed;

            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_micros(delay_micros)).await;
                // Guard check
                if s_seq.load(Ordering::SeqCst) == captured_seq
                    && *s_query.lock().unwrap() == captured_q
                {
                    *s_view.lock().unwrap() = vec![format!("result-for-{}", captured_q)];
                }
            });
        }
    }

    // Give in-flight tasks time to settle
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Finally, simulate user pressing clear
    let _final_seq = search_seq.fetch_add(1, Ordering::SeqCst) + 1;
    *cur_query.lock().unwrap() = String::new();
    *ui_view.lock().unwrap() = full_cache.clone();

    // Wait again to ensure NO late task overwrites the cleared UI view
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    assert_eq!(
        ui_view.lock().unwrap().len(),
        100,
        "UI view must remain exactly the full 100 cache entries after clear settles"
    );
    assert_eq!(*ui_view.lock().unwrap(), full_cache);
}

/// Stress test full-history SQLite discovery with 300 entries, including boundaries.
#[test]
fn test_adversarial_full_history_deep_discovery() {
    let repo = SqliteRepository::open_in_memory().expect("open sqlite");

    // Insert 300 entries: index 0 (oldest) to 299 (newest)
    for i in 0..300 {
        let content = match i {
            0 => "CRITICAL-ROOT-SECRET-000 oldest entry in db".to_string(),
            100 => "BOUNDARY-ENTRY-100 exactly on page 2 boundary".to_string(),
            199 => "SPECIAL_CHAR_%_LIKE_TEST entry with percent and underscore".to_string(),
            200 => "UNICODE_TÜRKÇE_TEST Türkçe karakterler: çığıöşü ÇIĞİÖŞÜ".to_string(),
            250 => "SQL_QUOTE_TEST entry with 'single' and \"double\" quotes".to_string(),
            _ => format!("Standard history record {:04} payload data", i),
        };
        let entry = make_test_entry(&content, Some("app"), i as i64);
        repo.insert_or_update(&entry).expect("insert");
    }

    // 1. In-memory GUI initial batch only retrieves 100 entries (indexes 200..299)
    let initial_batch = repo.list(100, 0, None, false).expect("list initial batch");
    assert_eq!(initial_batch.len(), 100);

    // Oldest entry (index 0) MUST NOT be in initial batch
    assert!(!initial_batch
        .iter()
        .any(|e| e.preview.contains("CRITICAL-ROOT-SECRET-000")));
    // Boundary entry (index 100) MUST NOT be in initial batch
    assert!(!initial_batch
        .iter()
        .any(|e| e.preview.contains("BOUNDARY-ENTRY-100")));
    // Special char entry (index 199) MUST NOT be in initial batch
    assert!(!initial_batch
        .iter()
        .any(|e| e.preview.contains("SPECIAL_CHAR_%_LIKE_TEST")));

    // 2. Full-history search discovers oldest entry (depth 300)
    let search_root = repo
        .search("CRITICAL-ROOT-SECRET-000", 100, 0)
        .expect("search root");
    assert_eq!(search_root.len(), 1);
    assert!(search_root[0].preview.contains("CRITICAL-ROOT-SECRET-000"));

    // 3. Full-history search discovers boundary entry (depth 200)
    let search_boundary = repo
        .search("BOUNDARY-ENTRY-100", 100, 0)
        .expect("search boundary");
    assert_eq!(search_boundary.len(), 1);
    assert!(search_boundary[0].preview.contains("BOUNDARY-ENTRY-100"));

    // 4. Special chars (% and _) matching
    let search_special = repo
        .search("SPECIAL_CHAR_%", 100, 0)
        .expect("search special");
    assert_eq!(search_special.len(), 1);

    // 5. Unicode Turkish character search
    let search_turkish = repo.search("çığıöşü", 100, 0).expect("search turkish");
    assert_eq!(search_turkish.len(), 1);
    assert!(search_turkish[0].preview.contains("Türkçe"));

    // 6. Quotes in search query (verifies FTS5 escaping: text.replace('"', "\"\""))
    let search_quotes = repo.search("\"double\"", 100, 0).expect("search quotes");
    assert_eq!(search_quotes.len(), 1);

    // 7. Verify GUI FuzzySearchEngine scoring and filtering on these results
    let engine = FuzzySearchEngine::new();
    let filtered_root = engine.filter_entries(
        &search_root,
        "CRITICAL-ROOT-SECRET-000",
        CategoryFilter::All,
    );
    assert_eq!(filtered_root.len(), 1);
}

/// Stress test zero-lag restoration: empty query executes synchronously in microseconds.
#[test]
fn test_adversarial_zero_lag_restoration_performance() {
    let engine = FuzzySearchEngine::new();

    // 1000 items in local cache
    let cached: Vec<ClipboardEntry> = (0..1000)
        .map(|i| make_test_entry(&format!("Cache record {:05}", i), Some("editor"), i as i64))
        .collect();

    // Simulate search clear with various empty/whitespace inputs
    let empty_inputs = vec!["", " ", "   ", "\t", "\n", " \t \r\n "];

    for input in empty_inputs {
        let start = Instant::now();
        let trimmed = input.trim();
        assert!(trimmed.is_empty(), "Must be recognized as empty query");

        let restored = engine.filter_entries(&cached, "", CategoryFilter::All);
        let elapsed = start.elapsed();

        assert_eq!(restored.len(), 1000, "Must restore entire cache");
        assert!(
            elapsed.as_millis() < 5,
            "Restoration must be sub-5ms (zero-lag), took {:?} for input {:?}",
            elapsed,
            input
        );
    }
}
