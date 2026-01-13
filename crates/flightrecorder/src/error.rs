//! Error types for flightrecorder.
//!
//! This module defines all error types used throughout the flightrecorder crate,
//! providing detailed context for debugging and user-friendly error messages.

use std::path::PathBuf;
use thiserror::Error;

/// The main error type for flightrecorder operations.
#[derive(Error, Debug)]
pub enum Error {
    // === Storage Errors ===
    /// Failed to open or create the database.
    #[error("failed to open database at {path}: {source}")]
    DatabaseOpen {
        /// Path to the database file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: rusqlite::Error,
    },

    /// A database query failed.
    #[error("database query failed: {0}")]
    DatabaseQuery(#[from] rusqlite::Error),

    /// Failed to run database migrations.
    #[error("database migration failed: {message}")]
    DatabaseMigration {
        /// Description of what went wrong.
        message: String,
    },

    // === Configuration Errors ===
    /// Failed to load configuration.
    #[error("failed to load configuration: {0}")]
    ConfigLoad(Box<figment::Error>),

    /// Configuration validation failed.
    #[error("invalid configuration: {message}")]
    ConfigValidation {
        /// Description of the validation failure.
        message: String,
    },

    // === Capture Errors ===
    /// A capture source failed to start.
    #[error("failed to start capture source '{name}': {message}")]
    CaptureSourceStart {
        /// Name of the capture source.
        name: &'static str,
        /// Description of what went wrong.
        message: String,
    },

    /// A capture source failed to stop.
    #[error("failed to stop capture source '{name}': {message}")]
    CaptureSourceStop {
        /// Name of the capture source.
        name: &'static str,
        /// Description of what went wrong.
        message: String,
    },

    /// Capture was filtered out by privacy rules.
    #[error("capture filtered: {reason}")]
    CaptureFiltered {
        /// Why the capture was filtered.
        reason: String,
    },

    // === IPC Errors ===
    /// Failed to connect to the daemon.
    #[error("failed to connect to daemon at {path}: {message}")]
    DaemonConnect {
        /// Path to the socket file.
        path: PathBuf,
        /// Description of what went wrong.
        message: String,
    },

    /// The daemon is not running.
    #[error("daemon is not running")]
    DaemonNotRunning,

    /// IPC communication failed.
    #[error("IPC error: {0}")]
    Ipc(String),

    // === Platform Errors ===
    /// Required platform permission is missing.
    #[error("missing permission: {permission}. {instructions}")]
    PermissionMissing {
        /// Name of the required permission.
        permission: String,
        /// Instructions for granting the permission.
        instructions: String,
    },

    /// Platform-specific operation failed.
    #[error("platform error: {0}")]
    Platform(String),

    // === I/O Errors ===
    /// File system operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to create a required directory.
    #[error("failed to create directory {path}: {source}")]
    DirectoryCreate {
        /// Path that couldn't be created.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },

    // === Serialization Errors ===
    /// JSON serialization/deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // === Generic Errors ===
    /// An operation timed out.
    #[error("operation timed out: {operation}")]
    Timeout {
        /// Description of the operation that timed out.
        operation: String,
    },

    /// An internal error occurred (bug).
    #[error("internal error: {0}")]
    Internal(String),
}

/// A specialized Result type for flightrecorder operations.
pub type Result<T> = std::result::Result<T, Error>;

impl From<figment::Error> for Error {
    fn from(err: figment::Error) -> Self {
        Self::ConfigLoad(Box::new(err))
    }
}

impl Error {
    /// Create a new platform error.
    #[must_use]
    pub fn platform(message: impl Into<String>) -> Self {
        Self::Platform(message.into())
    }

    /// Create a new internal error.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Create a new IPC error.
    #[must_use]
    pub fn ipc(message: impl Into<String>) -> Self {
        Self::Ipc(message.into())
    }

    /// Create a permission missing error with instructions.
    #[must_use]
    pub fn permission_missing(
        permission: impl Into<String>,
        instructions: impl Into<String>,
    ) -> Self {
        Self::PermissionMissing {
            permission: permission.into(),
            instructions: instructions.into(),
        }
    }

    /// Create a capture source start error.
    #[must_use]
    pub fn capture_source_start(name: &'static str, message: impl Into<String>) -> Self {
        Self::CaptureSourceStart {
            name,
            message: message.into(),
        }
    }

    /// Create a capture source stop error.
    #[must_use]
    pub fn capture_source_stop(name: &'static str, message: impl Into<String>) -> Self {
        Self::CaptureSourceStop {
            name,
            message: message.into(),
        }
    }

    /// Check if this error indicates the daemon is not running.
    #[must_use]
    pub fn is_daemon_not_running(&self) -> bool {
        matches!(self, Self::DaemonNotRunning)
    }

    /// Check if this error is a permission issue.
    #[must_use]
    pub fn is_permission_error(&self) -> bool {
        matches!(self, Self::PermissionMissing { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::DaemonNotRunning;
        assert_eq!(err.to_string(), "daemon is not running");

        let err = Error::platform("test error");
        assert_eq!(err.to_string(), "platform error: test error");
    }

    #[test]
    fn test_error_is_daemon_not_running() {
        assert!(Error::DaemonNotRunning.is_daemon_not_running());
        assert!(!Error::platform("test").is_daemon_not_running());
    }

    #[test]
    fn test_error_is_permission_error() {
        let err = Error::permission_missing("Accessibility", "Go to System Preferences");
        assert!(err.is_permission_error());
        assert!(!Error::DaemonNotRunning.is_permission_error());
    }

    #[test]
    fn test_permission_error_display() {
        let err = Error::permission_missing(
            "Accessibility API",
            "Grant access in System Preferences > Privacy > Accessibility",
        );
        let msg = err.to_string();
        assert!(msg.contains("Accessibility API"));
        assert!(msg.contains("System Preferences"));
    }

    #[test]
    fn test_internal_error() {
        let err = Error::internal("something went wrong");
        assert_eq!(err.to_string(), "internal error: something went wrong");
    }

    #[test]
    fn test_ipc_error() {
        let err = Error::ipc("connection refused");
        assert_eq!(err.to_string(), "IPC error: connection refused");
    }

    #[test]
    fn test_capture_source_start_error() {
        let err = Error::capture_source_start("clipboard", "failed to initialize");
        let msg = err.to_string();
        assert!(msg.contains("clipboard"));
        assert!(msg.contains("failed to initialize"));
    }

    #[test]
    fn test_capture_source_stop_error() {
        let err = Error::capture_source_stop("accessibility", "timeout");
        let msg = err.to_string();
        assert!(msg.contains("accessibility"));
        assert!(msg.contains("timeout"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_from_rusqlite_error() {
        // Create a rusqlite error by trying to open a non-existent DB in read-only mode
        let result = rusqlite::Connection::open_with_flags(
            "/nonexistent/path/db.sqlite",
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        );
        if let Err(sqlite_err) = result {
            let err: Error = sqlite_err.into();
            assert!(matches!(err, Error::DatabaseQuery(_)));
        }
    }

    #[test]
    fn test_from_json_error() {
        let json_result: std::result::Result<i32, serde_json::Error> =
            serde_json::from_str("not valid json");
        if let Err(json_err) = json_result {
            let err: Error = json_err.into();
            assert!(matches!(err, Error::Json(_)));
        }
    }

    #[test]
    fn test_database_migration_error_display() {
        let err = Error::DatabaseMigration {
            message: "version mismatch".to_string(),
        };
        assert!(err.to_string().contains("version mismatch"));
    }

    #[test]
    fn test_config_validation_error_display() {
        let err = Error::ConfigValidation {
            message: "invalid interval".to_string(),
        };
        assert!(err.to_string().contains("invalid interval"));
    }

    #[test]
    fn test_capture_filtered_error_display() {
        let err = Error::CaptureFiltered {
            reason: "contains password".to_string(),
        };
        assert!(err.to_string().contains("contains password"));
    }

    #[test]
    fn test_daemon_connect_error_display() {
        let err = Error::DaemonConnect {
            path: PathBuf::from("/tmp/test.sock"),
            message: "connection refused".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("/tmp/test.sock"));
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_timeout_error_display() {
        let err = Error::Timeout {
            operation: "database query".to_string(),
        };
        assert!(err.to_string().contains("database query"));
    }

    #[test]
    fn test_directory_create_error_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err = Error::DirectoryCreate {
            path: PathBuf::from("/root/forbidden"),
            source: io_err,
        };
        let msg = err.to_string();
        assert!(msg.contains("/root/forbidden"));
    }

    #[test]
    fn test_database_open_error_display() {
        let result = rusqlite::Connection::open_with_flags(
            "/nonexistent/path/db.sqlite",
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        );
        if let Err(sqlite_err) = result {
            let err = Error::DatabaseOpen {
                path: PathBuf::from("/nonexistent/path/db.sqlite"),
                source: sqlite_err,
            };
            let msg = err.to_string();
            assert!(msg.contains("/nonexistent/path/db.sqlite"));
        }
    }
}
