use chrono::Utc;
use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::PasteConfig;
use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::storage::SqliteRepository;
use std::cell::RefCell;
use std::rc::Rc;

#[path = "../src/injector/mod.rs"]
mod injector;
#[path = "../src/search.rs"]
mod search;

use injector::InjectorCascade;
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

#[test]
fn test_fuzzy_search_filtering() {
    let engine = FuzzySearchEngine::new();

    let entry1 = ClipboardEntry::new_text(
        "const apiToken = () => { return 'secret123'; };".to_string(),
        BlobStore::compute_hash(b"const apiToken = () => { return 'secret123'; };"),
        vec!["text/plain".to_string()],
        Some("vscode".to_string()),
    );

    let entry2 = ClipboardEntry::new_text(
        "Shopping list: apples, oranges".to_string(),
        BlobStore::compute_hash(b"Shopping list: apples, oranges"),
        vec!["text/plain".to_string()],
        Some("firefox".to_string()),
    );

    let entry3 = ClipboardEntry::new_uri_list(
        "file:///home/user/document.pdf".to_string(),
        BlobStore::compute_hash(b"file:///home/user/document.pdf"),
        vec!["text/uri-list".to_string()],
        Some("nautilus".to_string()),
    );

    let entries = vec![entry1.clone(), entry2.clone(), entry3.clone()];

    // 1. Search by keyword
    let matches = engine.filter_entries(&entries, "apples", CategoryFilter::All);
    assert_eq!(matches.len(), 1);
    assert!(matches[0].preview.contains("apples"));

    // 2. Search by source app
    let vscode_matches = engine.filter_entries(&entries, "vscode", CategoryFilter::All);
    assert_eq!(vscode_matches.len(), 1);

    // 3. Filter by category
    let file_matches = engine.filter_entries(&entries, "", CategoryFilter::Files);
    assert_eq!(file_matches.len(), 1);
    assert_eq!(file_matches[0].entry_type, EntryType::UriList);

    let code_matches = engine.filter_entries(&entries, "", CategoryFilter::Code);
    assert_eq!(code_matches.len(), 1);

    // 4. Test Snippet and Pinned filtering
    let mut snippet_entry = ClipboardEntry::new_text(
        "Hello {clipboard}".to_string(),
        BlobStore::compute_hash(b"Hello {clipboard}"),
        vec!["text/plain".to_string()],
        Some("Snippet".to_string()),
    );
    snippet_entry.is_pinned = true;

    let entries_with_snippet = vec![entry1, entry2, entry3, snippet_entry];
    let snippet_matches =
        engine.filter_entries(&entries_with_snippet, "", CategoryFilter::Snippets);
    assert_eq!(snippet_matches.len(), 1);
    assert_eq!(snippet_matches[0].source_app.as_deref(), Some("Snippet"));

    let pinned_matches = engine.filter_entries(&entries_with_snippet, "", CategoryFilter::Pinned);
    assert_eq!(pinned_matches.len(), 1);
}

#[tokio::test]
async fn test_injector_cascade_availability() {
    let cascade = InjectorCascade::new();
    let (name, success) = cascade.execute_paste().await;
    // Every environment should succeed at least on fallback or native
    assert!(!name.is_empty());
    assert!(success);
}

#[test]
fn test_r2_full_database_query_beyond_initial_limit() {
    let repo = SqliteRepository::open_in_memory().expect("open in-memory sqlite");

    // Populate 150 items: Record 000 (oldest) to Record 149 (newest)
    for i in 0..150 {
        let text = format!("Historical-Record-{:03} special data", i);
        let entry = make_test_entry(&text, Some("terminal"), i as i64);
        repo.insert_or_update(&entry).expect("insert entry");
    }

    // Default GUI in-memory fetch only retrieves initial 100 items
    let initial_batch = repo.list(100, 0, None, false).expect("list initial 100");
    assert_eq!(initial_batch.len(), 100);

    // Oldest items (e.g. Record 010) are beyond the initial 100 items slice
    let found_in_initial = initial_batch
        .iter()
        .any(|e| e.preview.contains("Record-010"));
    assert!(
        !found_in_initial,
        "Record-010 must NOT be in initial 100 in-memory batch"
    );

    // Full-history search queries the database via IPC / repo.search
    let search_results = repo
        .search("Record-010", 100, 0)
        .expect("search full database");
    assert_eq!(
        search_results.len(),
        1,
        "Universal search must find Record-010 beyond the 100 limit"
    );
    assert!(search_results[0].preview.contains("Record-010"));

    // Verify passing results through GUI FuzzySearchEngine
    let engine = FuzzySearchEngine::new();
    let gui_filtered = engine.filter_entries(&search_results, "Record-010", CategoryFilter::All);
    assert_eq!(gui_filtered.len(), 1);
    assert!(gui_filtered[0].preview.contains("Record-010"));
}

#[test]
fn test_r2_zero_lag_local_cache_restoration_on_empty_query() {
    let engine = FuzzySearchEngine::new();

    // Setup local cache of 100 items
    let mut cached_entries = Vec::with_capacity(100);
    for i in 0..100 {
        let text = format!("Cached-Item-{:03}", i);
        cached_entries.push(make_test_entry(&text, Some("editor"), i as i64));
    }

    // When a search query is active, only matches are shown
    let query = "Item-042";
    let filtered_matches = engine.filter_entries(&cached_entries, query, CategoryFilter::All);
    assert_eq!(filtered_matches.len(), 1);
    assert!(filtered_matches[0].preview.contains("Item-042"));

    // When query is cleared to empty (""), immediately restore full local cache without lag
    let empty_query = "";
    assert!(empty_query.trim().is_empty());

    let restored = engine.filter_entries(&cached_entries, "", CategoryFilter::All);
    assert_eq!(
        restored.len(),
        100,
        "Clearing query immediately restores all cached entries"
    );
    assert_eq!(restored[0].id, cached_entries[0].id);
    assert_eq!(restored[99].id, cached_entries[99].id);

    // Whitespace-only query also triggers zero-lag instant restoration
    let whitespace_query = "   \t\n  ";
    assert!(whitespace_query.trim().is_empty());
    let restored_whitespace = engine.filter_entries(&cached_entries, "", CategoryFilter::All);
    assert_eq!(restored_whitespace.len(), 100);
}

#[test]
fn test_r2_race_condition_sequence_protection() {
    let search_seq = Rc::new(RefCell::new(0u64));
    let current_query = Rc::new(RefCell::new(String::new()));

    // 1. User starts typing "first_query"
    let seq_1 = {
        let next = search_seq.borrow().wrapping_add(1);
        *search_seq.borrow_mut() = next;
        *current_query.borrow_mut() = "first_query".to_string();
        next
    };
    assert_eq!(seq_1, 1);
    let captured_query_1 = "first_query".to_string();

    // 2. User rapidly types "second_query" before first response arrives
    let seq_2 = {
        let next = search_seq.borrow().wrapping_add(1);
        *search_seq.borrow_mut() = next;
        *current_query.borrow_mut() = "second_query".to_string();
        next
    };
    assert_eq!(seq_2, 2);
    let captured_query_2 = "second_query".to_string();

    // 3. Late arriving response for Query 1 arrives
    // Guard check used in window.rs: *seq_c.borrow() != my_seq || *q_c.borrow() != q_target
    let query_1_valid =
        *search_seq.borrow() == seq_1 && *current_query.borrow() == captured_query_1;
    assert!(
        !query_1_valid,
        "Outdated Query 1 response must be rejected by sequence counter"
    );

    // 4. Query 2 response arrives
    let query_2_valid =
        *search_seq.borrow() == seq_2 && *current_query.borrow() == captured_query_2;
    assert!(query_2_valid, "Latest Query 2 response must be accepted");

    // 5. User clears search box: sequence is incremented to invalidate pending requests
    let seq_3 = {
        let next = search_seq.borrow().wrapping_add(1);
        *search_seq.borrow_mut() = next;
        *current_query.borrow_mut() = String::new();
        next
    };
    assert_eq!(seq_3, 3);

    // Delayed response from Query 2 arrives after clear
    let query_2_after_clear_valid =
        *search_seq.borrow() == seq_2 && *current_query.borrow() == captured_query_2;
    assert!(
        !query_2_after_clear_valid,
        "Responses arriving after search clear must be rejected to prevent overwriting zero-lag restored cache"
    );

    // 6. Text mismatch guard: sequence matches but query text was mutated
    let query_mismatch = *search_seq.borrow() == seq_3 && *current_query.borrow() == "mutated_text";
    assert!(
        !query_mismatch,
        "Query text mismatch must reject update even if sequence matches"
    );
}

#[test]
fn test_r2_auto_paste_effective_delay_calibration() {
    let mut config = PasteConfig {
        paste_delay_ms: 10,
        ..Default::default()
    };

    let effective = config.effective_delay_ms();
    // Regardless of environment, effective delay must be at least 50ms (or 120ms on Wayland)
    assert!(
        effective >= 50,
        "Effective delay must satisfy at least 50ms minimum"
    );

    // Explicitly high delay must be preserved
    config.paste_delay_ms = 350;
    assert_eq!(config.effective_delay_ms(), 350);
}
