//! SQLite-backed manifest — persistent record of every organized file.
//!
//! The manifest lives at `<output_dir>/.filemind/manifest.db`.
//! It records what was organized, when, and with what confidence so the
//! `status` subcommand can summarize past runs.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::error::{FileMindError, Result};

/// A single row in the `files` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub id: i64,
    pub session_id: i64,
    pub original_path: String,
    pub final_path: String,
    pub category: String,
    pub confidence: f32,
    pub tier_used: String,
    pub md5: String,
    pub sha256: String,
    pub organized_at: DateTime<Utc>,
}

/// Thread-safe handle to the manifest database.
#[derive(Clone)]
pub struct Manifest {
    conn: Arc<Mutex<Connection>>,
}

impl Manifest {
    /// Open (or create) the manifest database at `<output_dir>/.filemind/manifest.db`.
    ///
    /// Runs the schema migration on first open.
    pub fn open(output_dir: &Path) -> Result<Self> {
        let db_dir = output_dir.join(".filemind");
        std::fs::create_dir_all(&db_dir).map_err(FileMindError::Io)?;
        let db_path = db_dir.join("manifest.db");
        let conn = Connection::open(&db_path).map_err(FileMindError::Database)?;
        let manifest = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        manifest.migrate()?;
        Ok(manifest)
    }

    /// Run DDL migrations to ensure schema is up to date.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp   TEXT    NOT NULL,
                input_dir   TEXT    NOT NULL,
                output_dir  TEXT    NOT NULL,
                file_count  INTEGER NOT NULL DEFAULT 0,
                status      TEXT    NOT NULL DEFAULT 'active'
            );
            CREATE TABLE IF NOT EXISTS files (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id    INTEGER NOT NULL REFERENCES sessions(id),
                original_path TEXT    NOT NULL,
                final_path    TEXT    NOT NULL,
                category      TEXT    NOT NULL,
                confidence    REAL    NOT NULL,
                tier_used     TEXT    NOT NULL,
                md5           TEXT    NOT NULL,
                sha256        TEXT    NOT NULL,
                organized_at  TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_files_md5       ON files(md5);
            CREATE INDEX IF NOT EXISTS idx_files_session   ON files(session_id);
            CREATE INDEX IF NOT EXISTS idx_files_category  ON files(category);
            ",
        )
        .map_err(FileMindError::Database)
    }

    /// Returns `true` if a file with `md5` has already been organized.
    pub fn is_duplicate(&self, md5: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM files WHERE md5 = ?1",
            params![md5],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Insert a new file record into the manifest.
    pub fn insert_file(&self, entry: &NewEntry) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO files
             (session_id, original_path, final_path, category, confidence,
              tier_used, md5, sha256, organized_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.session_id,
                entry.original_path.to_string_lossy().as_ref(),
                entry.final_path.to_string_lossy().as_ref(),
                entry.category,
                entry.confidence,
                entry.tier_used,
                entry.md5,
                entry.sha256,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Create a new session record, returning its id.
    pub fn new_session(&self, input_dir: &Path, output_dir: &Path) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (timestamp, input_dir, output_dir, file_count, status)
             VALUES (?1, ?2, ?3, 0, 'active')",
            params![
                Utc::now().to_rfc3339(),
                input_dir.to_string_lossy().as_ref(),
                output_dir.to_string_lossy().as_ref(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Increment the file counter for `session_id`.
    pub fn increment_session_count(&self, session_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET file_count = file_count + 1 WHERE id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    /// Mark a session as completed.
    pub fn close_session(&self, session_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET status = 'completed' WHERE id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    /// Load all sessions ordered newest first.
    pub fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, input_dir, output_dir, file_count, status
             FROM sessions ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                input_dir: row.get(2)?,
                output_dir: row.get(3)?,
                file_count: row.get(4)?,
                status: row.get(5)?,
            })
        })?;
        rows.map(|r| r.map_err(FileMindError::Database))
            .collect()
    }

    /// Load all file entries for `session_id`.
    pub fn files_for_session(&self, session_id: i64) -> Result<Vec<ManifestEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, original_path, final_path, category,
                    confidence, tier_used, md5, sha256, organized_at
             FROM files WHERE session_id = ?1",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            let ts: String = row.get(9)?;
            let organized_at = ts
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            Ok(ManifestEntry {
                id: row.get(0)?,
                session_id: row.get(1)?,
                original_path: row.get(2)?,
                final_path: row.get(3)?,
                category: row.get(4)?,
                confidence: row.get(5)?,
                tier_used: row.get(6)?,
                md5: row.get(7)?,
                sha256: row.get(8)?,
                organized_at,
            })
        })?;
        rows.map(|r| r.map_err(FileMindError::Database))
            .collect()
    }

    /// Load category summary: (category, count, avg_confidence).
    pub fn category_summary(&self) -> Result<Vec<(String, i64, f64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) as cnt, AVG(confidence)
             FROM files GROUP BY category ORDER BY cnt DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, f64>(2)?))
        })?;
        rows.map(|r| r.map_err(FileMindError::Database))
            .collect()
    }

    /// Delete all file records for `session_id` (used by undo).
    pub fn delete_session_files(&self, session_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM files WHERE session_id = ?1",
            params![session_id],
        )?;
        conn.execute(
            "UPDATE sessions SET status = 'undone' WHERE id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    /// Find the most recent active session id.
    pub fn last_active_session(&self) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT id FROM sessions WHERE status = 'completed' ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(FileMindError::Database(e)),
        }
    }
}

/// Data needed to insert a new file record.
pub struct NewEntry {
    pub session_id: i64,
    pub original_path: PathBuf,
    pub final_path: PathBuf,
    pub category: String,
    pub confidence: f32,
    pub tier_used: String,
    pub md5: String,
    pub sha256: String,
}

/// A row from the `sessions` table.
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: i64,
    pub timestamp: String,
    pub input_dir: String,
    pub output_dir: String,
    pub file_count: i64,
    pub status: String,
}
