//! Database migration system for flightrecorder.
//!
//! This module handles database schema versioning and migrations,
//! ensuring the database schema stays up-to-date as the application evolves.

use rusqlite::Connection;

use crate::error::{Error, Result};

use super::schema::SCHEMA_STATEMENTS;

/// The current schema version.
pub const CURRENT_VERSION: i32 = 1;

/// Key used to store the schema version in the metadata table.
const VERSION_KEY: &str = "schema_version";

/// Initialize the database schema.
///
/// Creates all tables and indexes if they don't exist, then runs any
/// pending migrations to bring the schema up to the current version.
///
/// # Errors
///
/// Returns an error if schema creation or migration fails.
pub fn initialize_schema(conn: &Connection) -> Result<()> {
    // Create base schema
    for statement in SCHEMA_STATEMENTS {
        conn.execute(statement, [])?;
    }

    // Check and run migrations
    let version = get_schema_version(conn)?;
    if version < CURRENT_VERSION {
        run_migrations(conn, version)?;
    }

    Ok(())
}

/// Get the current schema version from the database.
///
/// Returns 0 if no version is set (fresh database).
fn get_schema_version(conn: &Connection) -> Result<i32> {
    let result: std::result::Result<String, rusqlite::Error> = conn.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [VERSION_KEY],
        |row| row.get(0),
    );

    match result {
        Ok(value) => value.parse().map_err(|_| Error::DatabaseMigration {
            message: format!("invalid schema version: {value}"),
        }),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(e.into()),
    }
}

/// Set the schema version in the database.
fn set_schema_version(conn: &Connection, version: i32) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
        (VERSION_KEY, version.to_string()),
    )?;
    Ok(())
}

/// Run migrations from the given version to the current version.
fn run_migrations(conn: &Connection, from_version: i32) -> Result<()> {
    let mut current = from_version;

    while current < CURRENT_VERSION {
        current += 1;
        run_migration(conn, current)?;
    }

    set_schema_version(conn, CURRENT_VERSION)?;
    Ok(())
}

/// Run a specific migration version.
fn run_migration(conn: &Connection, version: i32) -> Result<()> {
    match version {
        1 => migrate_v1(conn),
        _ => Err(Error::DatabaseMigration {
            message: format!("unknown migration version: {version}"),
        }),
    }
}

/// Migration to version 1 (initial schema).
///
/// This is a no-op since version 1 is the base schema created by `SCHEMA_STATEMENTS`.
fn migrate_v1(conn: &Connection) -> Result<()> {
    // Version 1 is the initial schema, which is already created.
    // Just set the version.
    set_schema_version(conn, 1)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_db() -> Connection {
        Connection::open_in_memory().expect("failed to create in-memory database")
    }

    #[test]
    fn test_initialize_schema_creates_tables() {
        let conn = create_test_db();
        initialize_schema(&conn).expect("failed to initialize schema");

        // Verify captures table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='captures'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify metadata table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='metadata'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_initialize_schema_sets_version() {
        let conn = create_test_db();
        initialize_schema(&conn).expect("failed to initialize schema");

        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, CURRENT_VERSION);
    }

    #[test]
    fn test_initialize_schema_idempotent() {
        let conn = create_test_db();

        // Initialize twice - should not error
        initialize_schema(&conn).expect("first init failed");
        initialize_schema(&conn).expect("second init failed");

        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, CURRENT_VERSION);
    }

    #[test]
    fn test_get_schema_version_fresh_db() {
        let conn = create_test_db();
        // Create just the metadata table
        conn.execute(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();

        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, 0);
    }

    #[test]
    fn test_set_and_get_schema_version() {
        let conn = create_test_db();
        conn.execute(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();

        set_schema_version(&conn, 42).unwrap();
        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, 42);
    }

    #[test]
    fn test_schema_version_update() {
        let conn = create_test_db();
        conn.execute(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();

        set_schema_version(&conn, 1).unwrap();
        set_schema_version(&conn, 2).unwrap();

        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn test_current_version_constant() {
        assert!(CURRENT_VERSION >= 1);
    }

    #[test]
    fn test_run_migration_unknown_version() {
        let conn = create_test_db();
        initialize_schema(&conn).unwrap();

        let result = run_migration(&conn, 999);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown migration version"));
    }

    #[test]
    fn test_indexes_created() {
        let conn = create_test_db();
        initialize_schema(&conn).expect("failed to initialize schema");

        // Check that indexes exist
        let indexes: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='captures'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(indexes.iter().any(|n| n.contains("timestamp")));
        assert!(indexes.iter().any(|n| n.contains("hash")));
        assert!(indexes.iter().any(|n| n.contains("app")));
        assert!(indexes.iter().any(|n| n.contains("type")));
    }
}
