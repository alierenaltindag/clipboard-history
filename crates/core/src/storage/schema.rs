pub const INITIAL_SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA mmap_size = 268435456;
PRAGMA temp_store = MEMORY;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS entries (
    id TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL UNIQUE,
    entry_type TEXT NOT NULL,
    preview TEXT NOT NULL,
    text_content TEXT,
    html_content TEXT,
    blob_hash TEXT,
    thumbnail_blob_hash TEXT,
    mime_types TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT NOT NULL,
    is_pinned INTEGER NOT NULL DEFAULT 0,
    source_app TEXT
);

CREATE INDEX IF NOT EXISTS idx_entries_last_used ON entries (last_used_at DESC);
CREATE INDEX IF NOT EXISTS idx_entries_is_pinned ON entries (is_pinned DESC);
CREATE INDEX IF NOT EXISTS idx_entries_pinned_last_used ON entries (is_pinned DESC, last_used_at DESC);
CREATE INDEX IF NOT EXISTS idx_entries_entry_type ON entries (entry_type);
CREATE INDEX IF NOT EXISTS idx_entries_content_hash ON entries (content_hash);

CREATE TABLE IF NOT EXISTS snippets (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    content TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'General',
    created_at TEXT NOT NULL,
    last_used_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_snippets_category ON snippets (category);
CREATE INDEX IF NOT EXISTS idx_snippets_last_used ON snippets (last_used_at DESC);
"#;
