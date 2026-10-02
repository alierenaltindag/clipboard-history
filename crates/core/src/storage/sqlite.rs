use crate::domain::{ClipboardEntry, EntryType, Snippet};
use crate::error::{CoreError, Result};
use crate::storage::schema::INITIAL_SCHEMA;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

#[derive(Clone)]
pub struct SqliteRepository {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl SqliteRepository {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }

        let is_new = !path.exists();
        let conn = Connection::open(path)?;

        if is_new {
            if let Ok(metadata) = fs::metadata(path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(path, perms);
            }
        }

        conn.execute_batch(INITIAL_SCHEMA)?;
        info!("Initialized SQLite database at {}", path.display());

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: path.to_path_buf(),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(INITIAL_SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: PathBuf::from(":memory:"),
        })
    }

    pub fn insert_or_update(&self, entry: &ClipboardEntry) -> Result<ClipboardEntry> {
        let conn = self.conn.lock().unwrap();

        // Check if content_hash already exists
        let existing_id: Option<String> = conn
            .query_row(
                "SELECT id FROM entries WHERE content_hash = ?1",
                params![&entry.content_hash],
                |row| row.get(0),
            )
            .ok();

        let now = Utc::now();
        if let Some(id) = existing_id {
            debug!(
                "Entry already exists (hash: {}), updating last_used_at",
                entry.content_hash
            );
            conn.execute(
                "UPDATE entries SET last_used_at = ?1 WHERE id = ?2",
                params![now.to_rfc3339(), &id],
            )?;

            drop(conn);
            return self.get_by_id(&id);
        }

        let mime_types_json = serde_json::to_string(&entry.mime_types)?;
        conn.execute(
            r#"
            INSERT INTO entries (
                id, content_hash, entry_type, preview, text_content, html_content,
                blob_hash, thumbnail_blob_hash, mime_types, size_bytes,
                created_at, last_used_at, is_pinned, source_app
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                &entry.id,
                &entry.content_hash,
                entry.entry_type.to_string(),
                &entry.preview,
                &entry.text_content,
                &entry.html_content,
                &entry.blob_hash,
                &entry.thumbnail_blob_hash,
                &mime_types_json,
                entry.size_bytes as i64,
                entry.created_at.to_rfc3339(),
                entry.last_used_at.to_rfc3339(),
                if entry.is_pinned { 1 } else { 0 },
                &entry.source_app,
            ],
        )?;

        Ok(entry.clone())
    }

    pub fn get_by_id(&self, id: &str) -> Result<ClipboardEntry> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, content_hash, entry_type, preview, text_content, html_content,
                   blob_hash, thumbnail_blob_hash, mime_types, size_bytes,
                   created_at, last_used_at, is_pinned, source_app
            FROM entries WHERE id = ?1 OR id LIKE ?1 || '%' LIMIT 1
            "#,
        )?;

        let entry = stmt.query_row(params![id], Self::map_row)?;
        Ok(entry)
    }

    pub fn get_by_hash(&self, content_hash: &str) -> Result<Option<ClipboardEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, content_hash, entry_type, preview, text_content, html_content,
                   blob_hash, thumbnail_blob_hash, mime_types, size_bytes,
                   created_at, last_used_at, is_pinned, source_app
            FROM entries WHERE content_hash = ?1
            "#,
        )?;

        match stmt.query_row(params![content_hash], Self::map_row) {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Database(e)),
        }
    }

    pub fn list(
        &self,
        limit: usize,
        offset: usize,
        entry_type_filter: Option<EntryType>,
        pinned_only: bool,
    ) -> Result<Vec<ClipboardEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut query = String::from(
            r#"
            SELECT id, content_hash, entry_type, preview, text_content, html_content,
                   blob_hash, thumbnail_blob_hash, mime_types, size_bytes,
                   created_at, last_used_at, is_pinned, source_app
            FROM entries
            WHERE 1=1
            "#,
        );

        let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if pinned_only {
            query.push_str(" AND is_pinned = 1");
        }

        if let Some(et) = entry_type_filter {
            query.push_str(" AND entry_type = ?");
            param_values.push(Box::new(et.to_string()));
        }

        query.push_str(" ORDER BY is_pinned DESC, last_used_at DESC LIMIT ? OFFSET ?");
        param_values.push(Box::new(limit as i64));
        param_values.push(Box::new(offset as i64));

        let mut stmt = conn.prepare(&query)?;
        let params_slice: Vec<&dyn rusqlite::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(&params_slice[..], Self::map_row)?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }

        Ok(results)
    }

    pub fn search(
        &self,
        query_text: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ClipboardEntry>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query_text);

        let mut stmt = conn.prepare(
            r#"
            SELECT id, content_hash, entry_type, preview, text_content, html_content,
                   blob_hash, thumbnail_blob_hash, mime_types, size_bytes,
                   created_at, last_used_at, is_pinned, source_app
            FROM entries
            WHERE preview LIKE ?1 OR text_content LIKE ?1 OR source_app LIKE ?1
            ORDER BY is_pinned DESC, last_used_at DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;

        let rows = stmt.query_map(params![pattern, limit as i64, offset as i64], Self::map_row)?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }

        Ok(results)
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "UPDATE entries SET is_pinned = ?1 WHERE id = ?2 OR id LIKE ?2 || '%'",
            params![if pinned { 1 } else { 0 }, id],
        )?;

        if affected == 0 {
            Err(CoreError::NotFound(id.to_string()))
        } else {
            Ok(())
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "DELETE FROM entries WHERE id = ?1 OR id LIKE ?1 || '%'",
            params![id],
        )?;
        if affected == 0 {
            Err(CoreError::NotFound(id.to_string()))
        } else {
            Ok(())
        }
    }

    pub fn clear(&self, include_pinned: bool) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let affected = if include_pinned {
            conn.execute("DELETE FROM entries", [])?
        } else {
            conn.execute("DELETE FROM entries WHERE is_pinned = 0", [])?
        };
        Ok(affected)
    }

    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn evict_expired(&self, cutoff_date: DateTime<Utc>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "DELETE FROM entries WHERE is_pinned = 0 AND last_used_at < ?1",
            params![cutoff_date.to_rfc3339()],
        )?;
        Ok(affected)
    }

    pub fn evict_capacity(&self, max_capacity: usize) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let total_unpinned: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE is_pinned = 0",
            [],
            |r| r.get(0),
        )?;

        if total_unpinned <= max_capacity as i64 {
            return Ok(0);
        }

        let excess = total_unpinned - (max_capacity as i64);
        let affected = conn.execute(
            r#"
            DELETE FROM entries
            WHERE id IN (
                SELECT id FROM entries
                WHERE is_pinned = 0
                ORDER BY last_used_at ASC
                LIMIT ?1
            )
            "#,
            params![excess],
        )?;

        Ok(affected)
    }

    pub fn get_all_blob_hashes(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT blob_hash FROM entries WHERE blob_hash IS NOT NULL
            UNION
            SELECT thumbnail_blob_hash FROM entries WHERE thumbnail_blob_hash IS NOT NULL
            "#,
        )?;

        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut hashes = Vec::new();
        for r in rows {
            hashes.push(r?);
        }
        Ok(hashes)
    }

    pub fn vacuum(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("VACUUM", [])?;
        Ok(())
    }

    pub fn insert_snippet(&self, snippet: &Snippet) -> Result<Snippet> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO snippets (id, label, content, category, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                snippet.id,
                snippet.label,
                snippet.content,
                snippet.category,
                snippet.created_at.to_rfc3339(),
                snippet.last_used_at.to_rfc3339(),
            ],
        )?;
        Ok(snippet.clone())
    }

    pub fn update_snippet(&self, snippet: &Snippet) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE snippets SET label = ?1, content = ?2, category = ?3, last_used_at = ?4 WHERE id = ?5",
            params![
                snippet.label,
                snippet.content,
                snippet.category,
                snippet.last_used_at.to_rfc3339(),
                snippet.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_snippet(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    pub fn get_snippet(&self, id: &str) -> Result<Option<Snippet>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, label, content, category, created_at, last_used_at FROM snippets WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_snippet_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_snippets(&self, category: Option<&str>) -> Result<Vec<Snippet>> {
        let conn = self.conn.lock().unwrap();
        let mut snippets = Vec::new();
        if let Some(cat) = category {
            let mut stmt = conn.prepare(
                "SELECT id, label, content, category, created_at, last_used_at FROM snippets WHERE category = ?1 ORDER BY last_used_at DESC",
            )?;
            let mut rows = stmt.query(params![cat])?;
            while let Some(row) = rows.next()? {
                snippets.push(Self::map_snippet_row(row)?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, label, content, category, created_at, last_used_at FROM snippets ORDER BY last_used_at DESC",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                snippets.push(Self::map_snippet_row(row)?);
            }
        }
        Ok(snippets)
    }

    pub fn touch_snippet(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE snippets SET last_used_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    fn map_snippet_row(row: &rusqlite::Row) -> rusqlite::Result<Snippet> {
        let id: String = row.get(0)?;
        let label: String = row.get(1)?;
        let content: String = row.get(2)?;
        let category: String = row.get(3)?;
        let created_at_str: String = row.get(4)?;
        let last_used_at_str: String = row.get(5)?;

        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let last_used_at = DateTime::parse_from_rfc3339(&last_used_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(Snippet {
            id,
            label,
            content,
            category,
            created_at,
            last_used_at,
        })
    }

    fn map_row(row: &rusqlite::Row) -> rusqlite::Result<ClipboardEntry> {
        let id: String = row.get(0)?;
        let content_hash: String = row.get(1)?;
        let entry_type_str: String = row.get(2)?;
        let preview: String = row.get(3)?;
        let text_content: Option<String> = row.get(4)?;
        let html_content: Option<String> = row.get(5)?;
        let blob_hash: Option<String> = row.get(6)?;
        let thumbnail_blob_hash: Option<String> = row.get(7)?;
        let mime_types_str: String = row.get(8)?;
        let size_bytes: i64 = row.get(9)?;
        let created_at_str: String = row.get(10)?;
        let last_used_at_str: String = row.get(11)?;
        let is_pinned_int: i32 = row.get(12)?;
        let source_app: Option<String> = row.get(13)?;

        let entry_type = EntryType::from_str(&entry_type_str).unwrap_or(EntryType::Text);
        let mime_types = serde_json::from_str(&mime_types_str).unwrap_or_default();
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let last_used_at = DateTime::parse_from_rfc3339(&last_used_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(ClipboardEntry {
            id,
            content_hash,
            entry_type,
            preview,
            text_content,
            html_content,
            blob_hash,
            thumbnail_blob_hash,
            mime_types,
            size_bytes: size_bytes as usize,
            created_at,
            last_used_at,
            is_pinned: is_pinned_int != 0,
            source_app,
        })
    }
}
