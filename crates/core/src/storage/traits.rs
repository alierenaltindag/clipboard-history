use crate::domain::{ClipboardEntry, EntryType, Snippet};
use crate::error::Result;
use chrono::{DateTime, Utc};

/// Abstract Storage interface for clipboard entry and snippet persistence.
/// Enables mock implementations in tests and decouples higher-level services from SQLite.
pub trait Storage: Send + Sync {
    fn insert_or_update(&self, entry: &ClipboardEntry) -> Result<ClipboardEntry>;
    fn get_by_id(&self, id: &str) -> Result<ClipboardEntry>;
    fn get_by_hash(&self, content_hash: &str) -> Result<Option<ClipboardEntry>>;
    fn list(
        &self,
        limit: usize,
        offset: usize,
        filter: Option<EntryType>,
        pinned_only: bool,
    ) -> Result<Vec<ClipboardEntry>>;
    fn search(&self, query: &str, limit: usize, offset: usize) -> Result<Vec<ClipboardEntry>>;
    fn set_pinned(&self, id: &str, pinned: bool) -> Result<()>;
    fn delete(&self, id: &str) -> Result<()>;
    fn batch_delete(&self, ids: &[String]) -> Result<usize>;
    fn batch_set_pinned(&self, ids: &[String], pinned: bool) -> Result<usize>;
    fn clear(&self, include_pinned: bool) -> Result<usize>;
    fn count(&self) -> Result<usize>;
    fn evict_expired(&self, cutoff_date: DateTime<Utc>) -> Result<usize>;
    fn evict_capacity(&self, max_capacity: usize) -> Result<usize>;
    fn get_all_blob_hashes(&self) -> Result<Vec<String>>;
    fn vacuum(&self) -> Result<()>;

    // Snippets
    fn insert_snippet(&self, snippet: &Snippet) -> Result<Snippet>;
    fn update_snippet(&self, snippet: &Snippet) -> Result<()>;
    fn delete_snippet(&self, id: &str) -> Result<bool>;
    fn get_snippet(&self, id: &str) -> Result<Option<Snippet>>;
    fn list_snippets(&self, category: Option<&str>) -> Result<Vec<Snippet>>;
    fn touch_snippet(&self, id: &str) -> Result<()>;
}
