use crate::domain::{ClipboardEntry, EntryType, Snippet};
use crate::error::{CoreError, Result};
use crate::search::parser::ParsedSearchQuery;
use crate::storage::schema::INITIAL_SCHEMA;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, TransactionBehavior};
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

        Self::init_connection(&conn)?;
        info!("Initialized SQLite database at {}", path.display());

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: path.to_path_buf(),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Returns the currently configured SQLite busy timeout in milliseconds.
    pub fn busy_timeout_ms(&self) -> Result<u64> {
        let conn = self.conn();
        let timeout: i64 = conn.query_row("PRAGMA busy_timeout", [], |r| r.get(0))?;
        Ok(timeout as u64)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_connection(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: PathBuf::from(":memory:"),
        })
    }

    fn init_connection(conn: &Connection) -> Result<()> {
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        let _ = conn.pragma_update(None, "busy_timeout", 5000);
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let _ = conn.pragma_update(None, "secure_delete", "ON");
        let _ = conn.pragma_update(None, "wal_autocheckpoint", 100);
        let _ = conn.pragma_update(None, "mmap_size", 0i64);
        let _ = conn.pragma_update(None, "cache_size", -1024);
        let _ = conn.pragma_update(None, "temp_store", "MEMORY");
        let _ = conn.pragma_update(None, "foreign_keys", "ON");
        conn.execute_batch(INITIAL_SCHEMA)?;
        let _ = conn.execute(
            "ALTER TABLE snippets ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 1",
            [],
        );
        Ok(())
    }

    #[inline]
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("Recovered from poisoned SQLite connection mutex");
            poisoned.into_inner()
        })
    }

    pub fn insert_or_update(&self, entry: &ClipboardEntry) -> Result<ClipboardEntry> {
        let conn = self.conn();

        // Check if content_hash already exists
        let existing: Option<(String, String, i64)> = conn
            .query_row(
                "SELECT id, last_used_at, is_pinned FROM entries WHERE content_hash = ?1",
                params![&entry.content_hash],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        if let Some((id, existing_last_used_str, _existing_pinned)) = existing {
            debug!(
                "Entry already exists (hash: {}), updating last_used_at if newer",
                entry.content_hash
            );
            let existing_last_used = chrono::DateTime::parse_from_rfc3339(&existing_last_used_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or(entry.last_used_at);

            if entry.last_used_at > existing_last_used {
                conn.execute(
                    "UPDATE entries SET last_used_at = ?1 WHERE id = ?2",
                    params![entry.last_used_at.to_rfc3339(), &id],
                )?;
            }

            // If incoming entry has blob hashes that existing was missing, populate them
            if entry.blob_hash.is_some() || entry.thumbnail_blob_hash.is_some() {
                let _ = conn.execute(
                    "UPDATE entries SET blob_hash = COALESCE(blob_hash, ?1), thumbnail_blob_hash = COALESCE(thumbnail_blob_hash, ?2) WHERE id = ?3",
                    params![&entry.blob_hash, &entry.thumbnail_blob_hash, &id],
                );
            }

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

    /// Batch inserts or updates clipboard entries within a single SQLite transaction.
    /// Slashes disk write amplification by >80% during rapid bursts of copy operations.
    pub fn batch_insert_or_update(
        &self,
        entries: &[ClipboardEntry],
    ) -> Result<Vec<ClipboardEntry>> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut results = Vec::with_capacity(entries.len());

        {
            let mut check_stmt = tx.prepare_cached(
                "SELECT id, last_used_at, is_pinned FROM entries WHERE content_hash = ?1",
            )?;
            let mut update_time_stmt =
                tx.prepare_cached("UPDATE entries SET last_used_at = ?1 WHERE id = ?2")?;
            let mut update_blob_stmt = tx.prepare_cached(
                "UPDATE entries SET blob_hash = COALESCE(blob_hash, ?1), thumbnail_blob_hash = COALESCE(thumbnail_blob_hash, ?2) WHERE id = ?3",
            )?;
            let mut insert_stmt = tx.prepare_cached(
                r#"
                INSERT INTO entries (
                    id, content_hash, entry_type, preview, text_content, html_content,
                    blob_hash, thumbnail_blob_hash, mime_types, size_bytes,
                    created_at, last_used_at, is_pinned, source_app
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                "#,
            )?;

            for entry in entries {
                let existing: Option<(String, String, i64)> = check_stmt
                    .query_row(params![&entry.content_hash], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .ok();

                if let Some((id, existing_last_used_str, _existing_pinned)) = existing {
                    let existing_last_used =
                        chrono::DateTime::parse_from_rfc3339(&existing_last_used_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or(entry.last_used_at);

                    if entry.last_used_at > existing_last_used {
                        update_time_stmt.execute(params![entry.last_used_at.to_rfc3339(), &id])?;
                    }

                    if entry.blob_hash.is_some() || entry.thumbnail_blob_hash.is_some() {
                        let _ = update_blob_stmt.execute(params![
                            &entry.blob_hash,
                            &entry.thumbnail_blob_hash,
                            &id
                        ]);
                    }

                    let mut updated_entry = entry.clone();
                    updated_entry.id = id;
                    results.push(updated_entry);
                } else {
                    let mime_types_json = serde_json::to_string(&entry.mime_types)?;
                    insert_stmt.execute(params![
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
                    ])?;
                    results.push(entry.clone());
                }
            }
        }

        tx.commit()?;
        Ok(results)
    }

    /// Stores a wrapped DEK and nonce for an entry to support crypto-shredding envelope encryption.
    pub fn insert_entry_dek(&self, entry_id: &str, wrapped_dek: &[u8], nonce: &[u8]) -> Result<()> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO entry_keys (entry_id, wrapped_dek, nonce, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![entry_id, wrapped_dek, nonce, now],
        )?;
        Ok(())
    }

    /// Retrieves the wrapped DEK and nonce for a given entry.
    pub fn get_entry_dek(&self, entry_id: &str) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
        let conn = self.conn();
        let res = conn.query_row(
            "SELECT wrapped_dek, nonce FROM entry_keys WHERE entry_id = ?1",
            params![entry_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match res {
            Ok(pair) => Ok(Some(pair)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Database(e)),
        }
    }

    /// Crypto-shreds an entry's DEK: securely deletes the key from entry_keys and truncates WAL.
    pub fn delete_entry_dek(&self, entry_id: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM entry_keys WHERE entry_id = ?1",
            params![entry_id],
        )?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> Result<ClipboardEntry> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err(CoreError::NotFound("Empty ID".to_string()));
        }

        let conn = self.conn();
        let is_prefix =
            trimmed.len() >= 6 && trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-');

        let mut stmt = if is_prefix {
            let pattern = format!(
                "{}%",
                trimmed
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            let mut s = conn.prepare(
                r#"
                SELECT id, content_hash, entry_type, preview, text_content, html_content,
                       blob_hash, thumbnail_blob_hash, mime_types, size_bytes,
                       created_at, last_used_at, is_pinned, source_app
                FROM entries
                WHERE id = ?1 OR id LIKE ?2 ESCAPE '\'
                ORDER BY last_used_at DESC
                LIMIT 1
                "#,
            )?;
            let entry = s.query_row(params![trimmed, pattern], Self::map_row)?;
            return Ok(entry);
        } else {
            conn.prepare(
                r#"
                SELECT id, content_hash, entry_type, preview, text_content, html_content,
                       blob_hash, thumbnail_blob_hash, mime_types, size_bytes,
                       created_at, last_used_at, is_pinned, source_app
                FROM entries WHERE id = ?1 LIMIT 1
                "#,
            )?
        };

        let entry = stmt.query_row(params![trimmed], Self::map_row)?;
        Ok(entry)
    }

    pub fn get_by_hash(&self, content_hash: &str) -> Result<Option<ClipboardEntry>> {
        let conn = self.conn();
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
        let conn = self.conn();
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
        let parsed = ParsedSearchQuery::parse(query_text);
        let conn = self.conn();

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

        if let Some(et) = parsed.entry_type {
            query.push_str(" AND entry_type = ?");
            param_values.push(Box::new(et.to_string()));
        }

        if let Some(app) = parsed.source_app {
            query.push_str(" AND LOWER(coalesce(source_app, '')) LIKE ?");
            param_values.push(Box::new(format!("%{}%", app.to_lowercase())));
        }

        if let Some(pinned) = parsed.is_pinned {
            query.push_str(" AND is_pinned = ?");
            param_values.push(Box::new(if pinned { 1 } else { 0 }));
        }

        if parsed.is_snippet {
            query.push_str(" AND source_app = 'Snippet'");
        }

        let text = parsed.text_query.trim();
        if !text.is_empty() {
            let fts_token = format!("\"{}\"*", text.replace('"', "\"\""));
            let pattern = format!("%{}%", text);
            query.push_str(
                " AND (id IN (SELECT id FROM entries_fts WHERE entries_fts MATCH ?) OR preview LIKE ? OR text_content LIKE ? OR source_app LIKE ?)"
            );
            param_values.push(Box::new(fts_token));
            param_values.push(Box::new(pattern.clone()));
            param_values.push(Box::new(pattern.clone()));
            param_values.push(Box::new(pattern));
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

    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err(CoreError::NotFound("Empty ID".to_string()));
        }
        let conn = self.conn();
        let is_prefix =
            trimmed.len() >= 6 && trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        let flag = if pinned { 1 } else { 0 };

        let affected = if is_prefix {
            let pattern = format!(
                "{}%",
                trimmed
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            conn.execute(
                "UPDATE entries SET is_pinned = ?1 WHERE id = ?2 OR id LIKE ?3 ESCAPE '\\'",
                params![flag, trimmed, pattern],
            )?
        } else {
            conn.execute(
                "UPDATE entries SET is_pinned = ?1 WHERE id = ?2",
                params![flag, trimmed],
            )?
        };

        if affected == 0 {
            // Also check snippets table so pinned canned templates can be toggled
            let snippet_affected = if is_prefix {
                let pattern = format!(
                    "{}%",
                    trimmed
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                );
                conn.execute(
                    "UPDATE snippets SET is_pinned = ?1 WHERE id = ?2 OR id LIKE ?3 ESCAPE '\\'",
                    params![flag, trimmed, pattern],
                )?
            } else {
                conn.execute(
                    "UPDATE snippets SET is_pinned = ?1 WHERE id = ?2",
                    params![flag, trimmed],
                )?
            };

            if snippet_affected == 0 {
                Err(CoreError::NotFound(id.to_string()))
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    pub fn set_pinned_by_hash(&self, content_hash: &str, pinned: bool) -> Result<()> {
        let trimmed = content_hash.trim();
        if trimmed.is_empty() {
            return Err(CoreError::NotFound("Empty content hash".to_string()));
        }
        let conn = self.conn();
        let flag = if pinned { 1 } else { 0 };
        let affected = conn.execute(
            "UPDATE entries SET is_pinned = ?1 WHERE content_hash = ?2",
            params![flag, trimmed],
        )?;
        if affected == 0 {
            Err(CoreError::NotFound(content_hash.to_string()))
        } else {
            Ok(())
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err(CoreError::NotFound("Empty ID".to_string()));
        }
        let conn = self.conn();
        let is_prefix =
            trimmed.len() >= 6 && trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-');

        let affected = if is_prefix {
            let pattern = format!(
                "{}%",
                trimmed
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            conn.execute(
                "DELETE FROM entries WHERE id = ?1 OR id LIKE ?2 ESCAPE '\\'",
                params![trimmed, pattern],
            )?
        } else {
            conn.execute("DELETE FROM entries WHERE id = ?1", params![trimmed])?
        };

        if affected == 0 {
            // Also check snippets table so canned templates can be deleted via general delete
            let snip_affected = if is_prefix {
                let pattern = format!(
                    "{}%",
                    trimmed
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                );
                conn.execute(
                    "DELETE FROM snippets WHERE id = ?1 OR id LIKE ?2 ESCAPE '\\'",
                    params![trimmed, pattern],
                )?
            } else {
                conn.execute("DELETE FROM snippets WHERE id = ?1", params![trimmed])?
            };

            if snip_affected == 0 {
                Err(CoreError::NotFound(id.to_string()))
            } else {
                Ok(())
            }
        } else {
            let _ = conn.execute(
                "DELETE FROM entry_keys WHERE entry_id = ?1",
                params![trimmed],
            );
            let _ = conn.execute_batch(
                "PRAGMA wal_checkpoint(TRUNCATE); INSERT INTO entries_fts(entries_fts) VALUES('optimize');",
            );
            Ok(())
        }
    }

    pub fn touch_entry(&self, id: &str) -> Result<()> {
        let conn = self.conn();
        let now = Utc::now().to_rfc3339();
        let trimmed = id.trim();
        conn.execute(
            "UPDATE entries SET last_used_at = ?1 WHERE id = ?2",
            params![now, trimmed],
        )?;
        Ok(())
    }

    pub fn batch_delete(&self, ids: &[String]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut deleted = 0;
        {
            let mut exact_stmt = tx.prepare_cached("DELETE FROM entries WHERE id = ?1")?;
            let mut prefix_stmt =
                tx.prepare_cached("DELETE FROM entries WHERE id = ?1 OR id LIKE ?2 ESCAPE '\\'")?;
            let mut key_stmt = tx.prepare_cached("DELETE FROM entry_keys WHERE entry_id = ?1")?;
            let mut exact_snip_stmt = tx.prepare_cached("DELETE FROM snippets WHERE id = ?1")?;
            let mut prefix_snip_stmt = tx.prepare_cached(
                "DELETE FROM snippets WHERE id = ?1 OR id LIKE ?2 ESCAPE '\\'",
            )?;

            for id in ids {
                let trimmed = id.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = key_stmt.execute(params![trimmed]);
                let is_prefix = trimmed.len() >= 6
                    && trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
                let count = if is_prefix {
                    let pattern = format!(
                        "{}%",
                        trimmed
                            .replace('\\', "\\\\")
                            .replace('%', "\\%")
                            .replace('_', "\\_")
                    );
                    let n = prefix_stmt.execute(params![trimmed, pattern])?;
                    if n == 0 {
                        prefix_snip_stmt.execute(params![trimmed, pattern])?
                    } else {
                        n
                    }
                } else {
                    let n = exact_stmt.execute(params![trimmed])?;
                    if n == 0 {
                        exact_snip_stmt.execute(params![trimmed])?
                    } else {
                        n
                    }
                };
                deleted += count;
            }
        }
        tx.commit()?;
        drop(conn);
        let c = self.conn();
        let _ = c.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE); INSERT INTO entries_fts(entries_fts) VALUES('optimize');",
        );
        Ok(deleted)
    }

    pub fn batch_set_pinned(&self, ids: &[String], pinned: bool) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut updated = 0;
        let flag = if pinned { 1 } else { 0 };
        {
            let mut exact_stmt =
                tx.prepare_cached("UPDATE entries SET is_pinned = ?1 WHERE id = ?2")?;
            let mut prefix_stmt = tx.prepare_cached(
                "UPDATE entries SET is_pinned = ?1 WHERE id = ?2 OR id LIKE ?3 ESCAPE '\\'",
            )?;
            let mut exact_snip_stmt =
                tx.prepare_cached("UPDATE snippets SET is_pinned = ?1 WHERE id = ?2")?;
            let mut prefix_snip_stmt = tx.prepare_cached(
                "UPDATE snippets SET is_pinned = ?1 WHERE id = ?2 OR id LIKE ?3 ESCAPE '\\'",
            )?;

            for id in ids {
                let trimmed = id.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let is_prefix = trimmed.len() >= 6
                    && trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
                let count = if is_prefix {
                    let pattern = format!(
                        "{}%",
                        trimmed
                            .replace('\\', "\\\\")
                            .replace('%', "\\%")
                            .replace('_', "\\_")
                    );
                    let n = prefix_stmt.execute(params![flag, trimmed, pattern])?;
                    if n == 0 {
                        prefix_snip_stmt.execute(params![flag, trimmed, pattern])?
                    } else {
                        n
                    }
                } else {
                    let n = exact_stmt.execute(params![flag, trimmed])?;
                    if n == 0 {
                        exact_snip_stmt.execute(params![flag, trimmed])?
                    } else {
                        n
                    }
                };
                updated += count;
            }
        }
        tx.commit()?;
        Ok(updated)
    }

    pub fn clear(&self, include_pinned: bool) -> Result<usize> {
        let conn = self.conn();
        let affected = if include_pinned {
            let _ = conn.execute("DELETE FROM entry_keys", []);
            conn.execute("DELETE FROM entries", [])?
        } else {
            let _ = conn.execute(
                "DELETE FROM entry_keys WHERE entry_id IN (SELECT id FROM entries WHERE is_pinned = 0)",
                [],
            );
            conn.execute("DELETE FROM entries WHERE is_pinned = 0", [])?
        };
        let _ = conn.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE); INSERT INTO entries_fts(entries_fts) VALUES('optimize');",
        );
        Ok(affected)
    }

    pub fn count(&self) -> Result<usize> {
        let conn = self.conn();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn evict_expired(&self, cutoff_date: DateTime<Utc>) -> Result<usize> {
        let conn = self.conn();
        let _ = conn.execute(
            "DELETE FROM entry_keys WHERE entry_id IN (SELECT id FROM entries WHERE is_pinned = 0 AND last_used_at < ?1)",
            params![cutoff_date.to_rfc3339()],
        );
        let affected = conn.execute(
            "DELETE FROM entries WHERE is_pinned = 0 AND last_used_at < ?1",
            params![cutoff_date.to_rfc3339()],
        )?;
        let _ = conn.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE); INSERT INTO entries_fts(entries_fts) VALUES('optimize');",
        );
        Ok(affected)
    }

    pub fn evict_capacity(&self, max_capacity: usize) -> Result<usize> {
        let conn = self.conn();
        let total_unpinned: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE is_pinned = 0",
            [],
            |r| r.get(0),
        )?;

        if total_unpinned <= max_capacity as i64 {
            return Ok(0);
        }

        let excess = total_unpinned - (max_capacity as i64);
        let _ = conn.execute(
            r#"
            DELETE FROM entry_keys
            WHERE entry_id IN (
                SELECT id FROM entries
                WHERE is_pinned = 0
                ORDER BY last_used_at ASC
                LIMIT ?1
            )
            "#,
            params![excess],
        );
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
        let _ = conn.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE); INSERT INTO entries_fts(entries_fts) VALUES('optimize');",
        );

        Ok(affected)
    }

    pub fn get_all_blob_hashes(&self) -> Result<Vec<String>> {
        let conn = self.conn();
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
        let conn = self.conn();
        conn.execute("VACUUM", [])?;
        Ok(())
    }

    pub fn insert_snippet(&self, snippet: &Snippet) -> Result<Snippet> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO snippets (id, label, content, category, created_at, last_used_at, is_pinned)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                snippet.id,
                snippet.label,
                snippet.content,
                snippet.category,
                snippet.created_at.to_rfc3339(),
                snippet.last_used_at.to_rfc3339(),
                if snippet.is_pinned { 1 } else { 0 },
            ],
        )?;
        Ok(snippet.clone())
    }

    pub fn update_snippet(&self, snippet: &Snippet) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE snippets SET label = ?1, content = ?2, category = ?3, last_used_at = ?4, is_pinned = ?5 WHERE id = ?6",
            params![
                snippet.label,
                snippet.content,
                snippet.category,
                snippet.last_used_at.to_rfc3339(),
                if snippet.is_pinned { 1 } else { 0 },
                snippet.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_snippet(&self, id: &str) -> Result<bool> {
        let conn = self.conn();
        let rows = conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    pub fn get_snippet(&self, id: &str) -> Result<Option<Snippet>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, label, content, category, created_at, last_used_at, is_pinned FROM snippets WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_snippet_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_snippets(&self, category: Option<&str>) -> Result<Vec<Snippet>> {
        let conn = self.conn();
        let mut snippets = Vec::new();
        if let Some(cat) = category {
            let mut stmt = conn.prepare(
                "SELECT id, label, content, category, created_at, last_used_at, is_pinned FROM snippets WHERE category = ?1 ORDER BY is_pinned DESC, last_used_at DESC",
            )?;
            let mut rows = stmt.query(params![cat])?;
            while let Some(row) = rows.next()? {
                snippets.push(Self::map_snippet_row(row)?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, label, content, category, created_at, last_used_at, is_pinned FROM snippets ORDER BY is_pinned DESC, last_used_at DESC",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                snippets.push(Self::map_snippet_row(row)?);
            }
        }
        Ok(snippets)
    }

    pub fn touch_snippet(&self, id: &str) -> Result<()> {
        let conn = self.conn();
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
        let is_pinned_int: i32 = row.get(6).unwrap_or(1);

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
            is_pinned: is_pinned_int != 0,
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

impl crate::storage::Storage for SqliteRepository {
    fn insert_or_update(&self, entry: &ClipboardEntry) -> Result<ClipboardEntry> {
        self.insert_or_update(entry)
    }

    fn get_by_id(&self, id: &str) -> Result<ClipboardEntry> {
        self.get_by_id(id)
    }

    fn get_by_hash(&self, content_hash: &str) -> Result<Option<ClipboardEntry>> {
        self.get_by_hash(content_hash)
    }

    fn list(
        &self,
        limit: usize,
        offset: usize,
        filter: Option<EntryType>,
        pinned_only: bool,
    ) -> Result<Vec<ClipboardEntry>> {
        self.list(limit, offset, filter, pinned_only)
    }

    fn search(&self, query: &str, limit: usize, offset: usize) -> Result<Vec<ClipboardEntry>> {
        self.search(query, limit, offset)
    }

    fn set_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        self.set_pinned(id, pinned)
    }

    fn set_pinned_by_hash(&self, content_hash: &str, pinned: bool) -> Result<()> {
        self.set_pinned_by_hash(content_hash, pinned)
    }

    fn delete(&self, id: &str) -> Result<()> {
        self.delete(id)
    }

    fn touch_entry(&self, id: &str) -> Result<()> {
        self.touch_entry(id)
    }

    fn batch_delete(&self, ids: &[String]) -> Result<usize> {
        self.batch_delete(ids)
    }

    fn batch_set_pinned(&self, ids: &[String], pinned: bool) -> Result<usize> {
        self.batch_set_pinned(ids, pinned)
    }

    fn clear(&self, include_pinned: bool) -> Result<usize> {
        self.clear(include_pinned)
    }

    fn count(&self) -> Result<usize> {
        self.count()
    }

    fn evict_expired(&self, cutoff_date: chrono::DateTime<chrono::Utc>) -> Result<usize> {
        self.evict_expired(cutoff_date)
    }

    fn evict_capacity(&self, max_capacity: usize) -> Result<usize> {
        self.evict_capacity(max_capacity)
    }

    fn get_all_blob_hashes(&self) -> Result<Vec<String>> {
        self.get_all_blob_hashes()
    }

    fn vacuum(&self) -> Result<()> {
        self.vacuum()
    }

    fn insert_snippet(&self, snippet: &Snippet) -> Result<Snippet> {
        self.insert_snippet(snippet)
    }

    fn update_snippet(&self, snippet: &Snippet) -> Result<()> {
        self.update_snippet(snippet)
    }

    fn delete_snippet(&self, id: &str) -> Result<bool> {
        self.delete_snippet(id)
    }

    fn get_snippet(&self, id: &str) -> Result<Option<Snippet>> {
        self.get_snippet(id)
    }

    fn list_snippets(&self, category: Option<&str>) -> Result<Vec<Snippet>> {
        self.list_snippets(category)
    }

    fn touch_snippet(&self, id: &str) -> Result<()> {
        self.touch_snippet(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_mutex_poison_recovery() {
        let repo = SqliteRepository::open_in_memory().unwrap();
        let conn_arc = repo.conn.clone();

        // Deliberately poison the mutex
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = conn_arc.lock().unwrap();
            panic!("Intentional test panic while holding connection lock");
        }));

        assert!(conn_arc.is_poisoned(), "Mutex must be poisoned");

        // SqliteRepository operations must recover and succeed
        let count = repo
            .count()
            .expect("repo.count() must recover from poisoned mutex");
        assert_eq!(count, 0);

        let vacuum = repo.vacuum();
        assert!(
            vacuum.is_ok(),
            "repo.vacuum() must recover from poisoned mutex"
        );
    }
}
