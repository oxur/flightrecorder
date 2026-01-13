//! `SQLite` schema definitions for flightrecorder.
//!
//! This module contains the SQL statements for creating and managing
//! the database schema.

/// SQL statement to create the captures table.
pub const CREATE_CAPTURES_TABLE: &str = r"
CREATE TABLE IF NOT EXISTS captures (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    source_app TEXT,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    capture_type TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
)
";

/// SQL statement to create an index on timestamp for efficient queries.
pub const CREATE_TIMESTAMP_INDEX: &str = r"
CREATE INDEX IF NOT EXISTS idx_captures_timestamp ON captures(timestamp DESC)
";

/// SQL statement to create an index on `content_hash` for deduplication.
pub const CREATE_HASH_INDEX: &str = r"
CREATE INDEX IF NOT EXISTS idx_captures_hash ON captures(content_hash)
";

/// SQL statement to create an index on `source_app` for filtering.
pub const CREATE_APP_INDEX: &str = r"
CREATE INDEX IF NOT EXISTS idx_captures_app ON captures(source_app)
";

/// SQL statement to create an index on `capture_type` for filtering.
pub const CREATE_TYPE_INDEX: &str = r"
CREATE INDEX IF NOT EXISTS idx_captures_type ON captures(capture_type)
";

/// SQL statement to create the metadata table for storing key-value pairs.
pub const CREATE_METADATA_TABLE: &str = r"
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
)
";

/// All schema creation statements in order.
pub const SCHEMA_STATEMENTS: &[&str] = &[
    CREATE_CAPTURES_TABLE,
    CREATE_TIMESTAMP_INDEX,
    CREATE_HASH_INDEX,
    CREATE_APP_INDEX,
    CREATE_TYPE_INDEX,
    CREATE_METADATA_TABLE,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_statements_not_empty() {
        assert!(!SCHEMA_STATEMENTS.is_empty());
        for stmt in SCHEMA_STATEMENTS {
            assert!(!stmt.is_empty());
        }
    }

    #[test]
    fn test_create_captures_table_contains_required_columns() {
        assert!(CREATE_CAPTURES_TABLE.contains("id INTEGER PRIMARY KEY"));
        assert!(CREATE_CAPTURES_TABLE.contains("timestamp TEXT NOT NULL"));
        assert!(CREATE_CAPTURES_TABLE.contains("content TEXT NOT NULL"));
        assert!(CREATE_CAPTURES_TABLE.contains("content_hash TEXT NOT NULL"));
        assert!(CREATE_CAPTURES_TABLE.contains("capture_type TEXT NOT NULL"));
    }

    #[test]
    fn test_create_metadata_table_structure() {
        assert!(CREATE_METADATA_TABLE.contains("key TEXT PRIMARY KEY"));
        assert!(CREATE_METADATA_TABLE.contains("value TEXT NOT NULL"));
    }
}
