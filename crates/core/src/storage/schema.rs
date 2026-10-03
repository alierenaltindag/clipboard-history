pub const INITIAL_SCHEMA: &str = r#"
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
CREATE INDEX IF NOT EXISTS idx_entries_blob_hash ON entries (blob_hash);
CREATE INDEX IF NOT EXISTS idx_entries_thumb_hash ON entries (thumbnail_blob_hash);

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

CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
    id UNINDEXED,
    preview,
    text_content,
    source_app,
    content='entries',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
  INSERT INTO entries_fts(rowid, id, preview, text_content, source_app)
  VALUES (new.rowid, new.id, new.preview, coalesce(new.text_content, ''), coalesce(new.source_app, ''));
END;

CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
  INSERT INTO entries_fts(entries_fts, rowid, id, preview, text_content, source_app)
  VALUES('delete', old.rowid, old.id, old.preview, coalesce(old.text_content, ''), coalesce(old.source_app, ''));
END;

CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE ON entries BEGIN
  INSERT INTO entries_fts(entries_fts, rowid, id, preview, text_content, source_app)
  VALUES('delete', old.rowid, old.id, old.preview, coalesce(old.text_content, ''), coalesce(old.source_app, ''));
  INSERT INTO entries_fts(rowid, id, preview, text_content, source_app)
  VALUES (new.rowid, new.id, new.preview, coalesce(new.text_content, ''), coalesce(new.source_app, ''));
END;
"#;
