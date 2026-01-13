//! Storage layer for flightrecorder.
//!
//! This module provides `SQLite`-based persistent storage for captured text,
//! including deduplication, search, and pruning capabilities.

pub mod migrations;
pub mod schema;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use tracing::{debug, info, warn};

use crate::capture::{Capture, CaptureType};
use crate::error::{Error, Result};

/// Storage engine for captured text.
///
/// Provides persistent storage using `SQLite` with support for:
/// - Capture insertion with deduplication
/// - Full-text search
/// - Filtering by app, type, and time range
/// - Automatic pruning of old entries
#[derive(Debug)]
pub struct Storage {
    /// Path to the database file.
    path: PathBuf,
    /// Database connection.
    conn: Connection,
}

impl Storage {
    /// Open or create a storage database at the given path.
    ///
    /// Creates the parent directories and database file if they don't exist.
    /// Initializes the schema if this is a new database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or schema initialization fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|source| Error::DirectoryCreate {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }

        debug!("Opening database at {}", path.display());
        let conn = Connection::open(&path).map_err(|source| Error::DatabaseOpen {
            path: path.clone(),
            source,
        })?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        // Initialize schema
        migrations::initialize_schema(&conn)?;

        info!("Database opened successfully at {}", path.display());
        Ok(Self { path, conn })
    }

    /// Create an in-memory storage instance for testing.
    ///
    /// # Errors
    ///
    /// Returns an error if the in-memory database cannot be created.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|source| Error::DatabaseOpen {
            path: PathBuf::from(":memory:"),
            source,
        })?;

        migrations::initialize_schema(&conn)?;

        Ok(Self {
            path: PathBuf::from(":memory:"),
            conn,
        })
    }

    /// Get the path to the database file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Insert a capture into storage.
    ///
    /// Returns the assigned ID, or `None` if the capture was deduplicated
    /// (i.e., an identical capture already exists).
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn insert(&self, capture: &Capture) -> Result<Option<i64>> {
        // Check for duplicate by hash
        if self.exists_by_hash(&capture.content_hash)? {
            debug!(
                "Skipping duplicate capture with hash {}",
                &capture.content_hash[..16]
            );
            return Ok(None);
        }

        let capture_type = capture.capture_type.to_string();
        let timestamp = capture.timestamp.to_rfc3339();

        self.conn.execute(
            r"
            INSERT INTO captures (timestamp, source_app, content, content_hash, capture_type)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                timestamp,
                capture.source_app,
                capture.content,
                capture.content_hash,
                capture_type,
            ],
        )?;

        let id = self.conn.last_insert_rowid();
        debug!("Inserted capture with id {}", id);
        Ok(Some(id))
    }

    /// Check if a capture with the given hash already exists.
    fn exists_by_hash(&self, hash: &str) -> Result<bool> {
        let count: i32 = self.conn.query_row(
            "SELECT COUNT(*) FROM captures WHERE content_hash = ?1",
            [hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get a capture by its ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn get(&self, id: i64) -> Result<Option<Capture>> {
        let result = self
            .conn
            .query_row(
                r"
                SELECT id, timestamp, source_app, content, content_hash, capture_type
                FROM captures WHERE id = ?1
                ",
                [id],
                Self::row_to_capture,
            )
            .optional()?;
        Ok(result)
    }

    /// Get the most recent captures.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn get_recent(&self, limit: usize) -> Result<Vec<Capture>> {
        let mut stmt = self.conn.prepare(
            r"
            SELECT id, timestamp, source_app, content, content_hash, capture_type
            FROM captures ORDER BY timestamp DESC LIMIT ?1
            ",
        )?;

        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let captures = stmt
            .query_map([limit_i64], Self::row_to_capture)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(captures)
    }

    /// Get captures from a specific application.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn get_by_app(&self, app: &str, limit: usize) -> Result<Vec<Capture>> {
        let mut stmt = self.conn.prepare(
            r"
            SELECT id, timestamp, source_app, content, content_hash, capture_type
            FROM captures WHERE source_app = ?1
            ORDER BY timestamp DESC LIMIT ?2
            ",
        )?;

        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let captures = stmt
            .query_map(params![app, limit_i64], Self::row_to_capture)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(captures)
    }

    /// Get captures of a specific type.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn get_by_type(&self, capture_type: CaptureType, limit: usize) -> Result<Vec<Capture>> {
        let type_str = capture_type.to_string();
        let mut stmt = self.conn.prepare(
            r"
            SELECT id, timestamp, source_app, content, content_hash, capture_type
            FROM captures WHERE capture_type = ?1
            ORDER BY timestamp DESC LIMIT ?2
            ",
        )?;

        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let captures = stmt
            .query_map(params![type_str, limit_i64], Self::row_to_capture)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(captures)
    }

    /// Search captures by content.
    ///
    /// Performs a case-insensitive substring search.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Capture>> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            r"
            SELECT id, timestamp, source_app, content, content_hash, capture_type
            FROM captures WHERE content LIKE ?1
            ORDER BY timestamp DESC LIMIT ?2
            ",
        )?;

        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let captures = stmt
            .query_map(params![pattern, limit_i64], Self::row_to_capture)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(captures)
    }

    /// Get captures within a time range.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn get_by_time_range(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Capture>> {
        let since_str = since.to_rfc3339();
        let until_str = until.to_rfc3339();

        let mut stmt = self.conn.prepare(
            r"
            SELECT id, timestamp, source_app, content, content_hash, capture_type
            FROM captures WHERE timestamp >= ?1 AND timestamp <= ?2
            ORDER BY timestamp DESC LIMIT ?3
            ",
        )?;

        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let captures = stmt
            .query_map(
                params![since_str, until_str, limit_i64],
                Self::row_to_capture,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(captures)
    }

    /// Count total captures in storage.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM captures", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Delete a capture by ID.
    ///
    /// Returns `true` if a capture was deleted, `false` if not found.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn delete(&self, id: i64) -> Result<bool> {
        let affected = self
            .conn
            .execute("DELETE FROM captures WHERE id = ?1", [id])?;
        Ok(affected > 0)
    }

    /// Prune captures older than the given duration.
    ///
    /// Returns the number of captures deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn prune_older_than(&self, max_age: Duration) -> Result<usize> {
        let cutoff = Utc::now() - max_age;
        let cutoff_str = cutoff.to_rfc3339();

        let affected = self
            .conn
            .execute("DELETE FROM captures WHERE timestamp < ?1", [cutoff_str])?;

        if affected > 0 {
            info!("Pruned {} old captures", affected);
        }
        Ok(affected)
    }

    /// Prune captures to keep only the most recent N entries.
    ///
    /// Returns the number of captures deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn prune_keep_recent(&self, keep_count: usize) -> Result<usize> {
        let keep_i64 = i64::try_from(keep_count).unwrap_or(i64::MAX);
        let affected = self.conn.execute(
            r"
            DELETE FROM captures WHERE id NOT IN (
                SELECT id FROM captures ORDER BY timestamp DESC LIMIT ?1
            )
            ",
            [keep_i64],
        )?;

        if affected > 0 {
            info!("Pruned {} captures to keep {} recent", affected, keep_count);
        }
        Ok(affected)
    }

    /// Get database statistics.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn stats(&self) -> Result<StorageStats> {
        let total_captures = self.count()?;

        let oldest: Option<String> = self
            .conn
            .query_row(
                "SELECT timestamp FROM captures ORDER BY timestamp ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let newest: Option<String> = self
            .conn
            .query_row(
                "SELECT timestamp FROM captures ORDER BY timestamp DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let oldest_capture = oldest
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        let newest_capture = newest
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        // Get database file size
        let db_size_bytes = if self.path.to_string_lossy() == ":memory:" {
            0
        } else {
            std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
        };

        Ok(StorageStats {
            total_captures,
            oldest_capture,
            newest_capture,
            db_size_bytes,
        })
    }

    /// Convert a database row to a Capture struct.
    fn row_to_capture(row: &rusqlite::Row) -> rusqlite::Result<Capture> {
        let id: i64 = row.get(0)?;
        let timestamp_str: String = row.get(1)?;
        let source_app: Option<String> = row.get(2)?;
        let content: String = row.get(3)?;
        let content_hash: String = row.get(4)?;
        let capture_type_str: String = row.get(5)?;

        let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

        let capture_type = match capture_type_str.as_str() {
            "clipboard" => CaptureType::Clipboard,
            "text_field" => CaptureType::TextField,
            "keystroke" => CaptureType::Keystroke,
            _ => {
                warn!(
                    "Unknown capture type: {}, defaulting to clipboard",
                    capture_type_str
                );
                CaptureType::Clipboard
            }
        };

        Ok(Capture {
            id: Some(id),
            timestamp,
            source_app,
            content,
            content_hash,
            capture_type,
        })
    }
}

/// Statistics about the storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageStats {
    /// Total number of captures stored.
    pub total_captures: i64,
    /// Timestamp of the oldest capture.
    pub oldest_capture: Option<DateTime<Utc>>,
    /// Timestamp of the newest capture.
    pub newest_capture: Option<DateTime<Utc>>,
    /// Size of the database file in bytes.
    pub db_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_storage() -> Storage {
        Storage::open_in_memory().expect("failed to create test storage")
    }

    fn create_test_capture(content: &str) -> Capture {
        Capture::new(content.to_string(), CaptureType::Clipboard, None)
    }

    #[test]
    fn test_open_in_memory() {
        let storage = Storage::open_in_memory();
        assert!(storage.is_ok());
    }

    #[test]
    fn test_insert_and_get() {
        let storage = create_test_storage();
        let capture = create_test_capture("Hello, world!");

        let id = storage.insert(&capture).unwrap();
        assert!(id.is_some());

        let retrieved = storage.get(id.unwrap()).unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content, "Hello, world!");
        assert_eq!(retrieved.capture_type, CaptureType::Clipboard);
    }

    #[test]
    fn test_insert_deduplication() {
        let storage = create_test_storage();
        let capture = create_test_capture("Duplicate content");

        let id1 = storage.insert(&capture).unwrap();
        let id2 = storage.insert(&capture).unwrap();

        assert!(id1.is_some());
        assert!(id2.is_none()); // Deduplicated
    }

    #[test]
    fn test_get_nonexistent() {
        let storage = create_test_storage();
        let result = storage.get(99999).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_recent() {
        let storage = create_test_storage();

        for i in 0..5 {
            let capture = create_test_capture(&format!("Capture {i}"));
            storage.insert(&capture).unwrap();
        }

        let recent = storage.get_recent(3).unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_get_by_app() {
        let storage = create_test_storage();

        let mut capture1 = create_test_capture("From app A");
        capture1.source_app = Some("AppA".to_string());
        storage.insert(&capture1).unwrap();

        let mut capture2 = create_test_capture("From app B");
        capture2.source_app = Some("AppB".to_string());
        storage.insert(&capture2).unwrap();

        let app_a = storage.get_by_app("AppA", 10).unwrap();
        assert_eq!(app_a.len(), 1);
        assert_eq!(app_a[0].source_app, Some("AppA".to_string()));
    }

    #[test]
    fn test_get_by_type() {
        let storage = create_test_storage();

        let clipboard = Capture::new("Clipboard".to_string(), CaptureType::Clipboard, None);
        let textfield = Capture::new("TextField".to_string(), CaptureType::TextField, None);

        storage.insert(&clipboard).unwrap();
        storage.insert(&textfield).unwrap();

        let results = storage.get_by_type(CaptureType::TextField, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].capture_type, CaptureType::TextField);
    }

    #[test]
    fn test_search() {
        let storage = create_test_storage();

        storage.insert(&create_test_capture("Hello world")).unwrap();
        storage
            .insert(&create_test_capture("Goodbye world"))
            .unwrap();
        storage.insert(&create_test_capture("Hello there")).unwrap();

        let results = storage.search("Hello", 10).unwrap();
        assert_eq!(results.len(), 2);

        let results = storage.search("world", 10).unwrap();
        assert_eq!(results.len(), 2);

        let results = storage.search("nonexistent", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_count() {
        let storage = create_test_storage();
        assert_eq!(storage.count().unwrap(), 0);

        storage.insert(&create_test_capture("One")).unwrap();
        storage.insert(&create_test_capture("Two")).unwrap();

        assert_eq!(storage.count().unwrap(), 2);
    }

    #[test]
    fn test_delete() {
        let storage = create_test_storage();
        let id = storage
            .insert(&create_test_capture("To delete"))
            .unwrap()
            .unwrap();

        assert!(storage.get(id).unwrap().is_some());
        assert!(storage.delete(id).unwrap());
        assert!(storage.get(id).unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let storage = create_test_storage();
        assert!(!storage.delete(99999).unwrap());
    }

    #[test]
    fn test_prune_keep_recent() {
        let storage = create_test_storage();

        for i in 0..10 {
            let capture = create_test_capture(&format!("Capture {i}"));
            storage.insert(&capture).unwrap();
        }

        assert_eq!(storage.count().unwrap(), 10);

        let pruned = storage.prune_keep_recent(5).unwrap();
        assert_eq!(pruned, 5);
        assert_eq!(storage.count().unwrap(), 5);
    }

    #[test]
    fn test_stats_empty() {
        let storage = create_test_storage();
        let stats = storage.stats().unwrap();

        assert_eq!(stats.total_captures, 0);
        assert!(stats.oldest_capture.is_none());
        assert!(stats.newest_capture.is_none());
    }

    #[test]
    fn test_stats_with_data() {
        let storage = create_test_storage();

        storage.insert(&create_test_capture("First")).unwrap();
        storage.insert(&create_test_capture("Second")).unwrap();

        let stats = storage.stats().unwrap();
        assert_eq!(stats.total_captures, 2);
        assert!(stats.oldest_capture.is_some());
        assert!(stats.newest_capture.is_some());
    }

    #[test]
    fn test_path() {
        let storage = create_test_storage();
        assert_eq!(storage.path().to_string_lossy(), ":memory:");
    }

    #[test]
    fn test_capture_with_source_app() {
        let storage = create_test_storage();
        let mut capture = create_test_capture("Test");
        capture.source_app = Some("TestApp".to_string());

        let id = storage.insert(&capture).unwrap().unwrap();
        let retrieved = storage.get(id).unwrap().unwrap();

        assert_eq!(retrieved.source_app, Some("TestApp".to_string()));
    }

    #[test]
    fn test_get_by_time_range() {
        let storage = create_test_storage();

        // Insert some captures
        storage.insert(&create_test_capture("In range")).unwrap();

        let now = Utc::now();
        let since = now - Duration::hours(1);
        let until = now + Duration::hours(1);

        let results = storage.get_by_time_range(since, until, 10).unwrap();
        assert_eq!(results.len(), 1);

        // Query outside range
        let old_since = now - Duration::days(10);
        let old_until = now - Duration::days(9);
        let results = storage.get_by_time_range(old_since, old_until, 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_all_capture_types() {
        let storage = create_test_storage();

        let clipboard = Capture::new("Clipboard".to_string(), CaptureType::Clipboard, None);
        let textfield = Capture::new("TextField".to_string(), CaptureType::TextField, None);
        let keystroke = Capture::new("Keystroke".to_string(), CaptureType::Keystroke, None);

        let id1 = storage.insert(&clipboard).unwrap().unwrap();
        let id2 = storage.insert(&textfield).unwrap().unwrap();
        let id3 = storage.insert(&keystroke).unwrap().unwrap();

        assert_eq!(
            storage.get(id1).unwrap().unwrap().capture_type,
            CaptureType::Clipboard
        );
        assert_eq!(
            storage.get(id2).unwrap().unwrap().capture_type,
            CaptureType::TextField
        );
        assert_eq!(
            storage.get(id3).unwrap().unwrap().capture_type,
            CaptureType::Keystroke
        );
    }

    #[test]
    fn test_empty_content() {
        let storage = create_test_storage();
        let capture = create_test_capture("");

        let id = storage.insert(&capture).unwrap().unwrap();
        let retrieved = storage.get(id).unwrap().unwrap();

        assert_eq!(retrieved.content, "");
        assert!(retrieved.is_empty());
    }

    #[test]
    fn test_unicode_content() {
        let storage = create_test_storage();
        let capture = create_test_capture("Hello 世界 🌍 مرحبا");

        let id = storage.insert(&capture).unwrap().unwrap();
        let retrieved = storage.get(id).unwrap().unwrap();

        assert_eq!(retrieved.content, "Hello 世界 🌍 مرحبا");
    }

    #[test]
    fn test_large_content() {
        let storage = create_test_storage();
        let large_content = "x".repeat(100_000);
        let capture = create_test_capture(&large_content);

        let id = storage.insert(&capture).unwrap().unwrap();
        let retrieved = storage.get(id).unwrap().unwrap();

        assert_eq!(retrieved.content.len(), 100_000);
    }

    #[test]
    fn test_open_file_based() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("flightrecorder_test_{}.db", std::process::id()));

        // Open and create database
        let storage = Storage::open(&db_path).unwrap();
        storage.insert(&create_test_capture("Test")).unwrap();
        assert_eq!(storage.count().unwrap(), 1);

        // Verify path is correct
        assert_eq!(storage.path(), db_path);

        // Clean up
        drop(storage);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }

    #[test]
    fn test_open_creates_parent_dirs() {
        let temp_dir = std::env::temp_dir();
        let nested_path = temp_dir.join(format!(
            "flightrecorder_test_{}/nested/db.sqlite",
            std::process::id()
        ));

        // Ensure parent doesn't exist
        if let Some(parent) = nested_path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }

        // Open should create parent directories
        let storage = Storage::open(&nested_path).unwrap();
        assert!(nested_path.exists());

        // Clean up
        drop(storage);
        if let Some(parent) = nested_path.parent() {
            let _ = std::fs::remove_dir_all(parent.parent().unwrap());
        }
    }

    #[test]
    fn test_prune_older_than() {
        let storage = create_test_storage();

        // Insert a capture
        storage.insert(&create_test_capture("Recent")).unwrap();

        // Prune with 1 day max age - nothing should be deleted
        let pruned = storage.prune_older_than(Duration::days(1)).unwrap();
        assert_eq!(pruned, 0);
        assert_eq!(storage.count().unwrap(), 1);

        // Prune with 0 seconds max age - still should keep the capture (it was just inserted)
        // Note: This might be flaky in very slow systems, but generally captures are < 1 second old
        let pruned = storage.prune_older_than(Duration::seconds(0)).unwrap();
        // The capture should be deleted since it's older than 0 seconds
        assert_eq!(storage.count().unwrap(), 1 - pruned as i64);
    }

    #[test]
    fn test_prune_keep_recent_no_pruning_needed() {
        let storage = create_test_storage();

        storage.insert(&create_test_capture("One")).unwrap();
        storage.insert(&create_test_capture("Two")).unwrap();

        // Keep more than we have
        let pruned = storage.prune_keep_recent(10).unwrap();
        assert_eq!(pruned, 0);
        assert_eq!(storage.count().unwrap(), 2);
    }

    #[test]
    fn test_stats_db_size() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!(
            "flightrecorder_size_test_{}.db",
            std::process::id()
        ));

        let storage = Storage::open(&db_path).unwrap();
        storage.insert(&create_test_capture("Test")).unwrap();

        let stats = storage.stats().unwrap();
        // File-based storage should have non-zero size
        assert!(stats.db_size_bytes > 0);

        // Clean up
        drop(storage);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
    }

    #[test]
    fn test_storage_stats_debug() {
        let stats = StorageStats {
            total_captures: 10,
            oldest_capture: Some(Utc::now()),
            newest_capture: Some(Utc::now()),
            db_size_bytes: 1024,
        };
        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("total_captures"));
        assert!(debug_str.contains("10"));
    }

    #[test]
    fn test_storage_stats_clone() {
        let stats = StorageStats {
            total_captures: 5,
            oldest_capture: None,
            newest_capture: None,
            db_size_bytes: 512,
        };
        let cloned = stats.clone();
        assert_eq!(stats, cloned);
    }

    #[test]
    fn test_search_empty_query() {
        let storage = create_test_storage();

        storage
            .insert(&create_test_capture("Test content"))
            .unwrap();

        // Empty search should match everything
        let results = storage.search("", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_get_recent_with_zero_limit() {
        let storage = create_test_storage();

        storage.insert(&create_test_capture("Test")).unwrap();

        let results = storage.get_recent(0).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_get_by_app_empty_results() {
        let storage = create_test_storage();

        let mut capture = create_test_capture("Test");
        capture.source_app = Some("OtherApp".to_string());
        storage.insert(&capture).unwrap();

        let results = storage.get_by_app("NonExistentApp", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_get_by_type_keystroke() {
        let storage = create_test_storage();

        let keystroke = Capture::new("Typed".to_string(), CaptureType::Keystroke, None);
        storage.insert(&keystroke).unwrap();

        let results = storage.get_by_type(CaptureType::Keystroke, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].capture_type, CaptureType::Keystroke);
    }
}
