use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::domain::{ClipboardEntry, EntryType};

#[path = "../src/injector/mod.rs"]
mod injector;
#[path = "../src/search.rs"]
mod search;

use injector::InjectorCascade;
use search::{CategoryFilter, FuzzySearchEngine};

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
