//! Logging configuration for flightrecorder.
//!
//! This module provides initialization and configuration for the tracing-based
//! logging system used throughout flightrecorder.

use tracing::Level;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Verbosity level for logging output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
    /// Suppress all output except errors.
    Quiet,
    /// Normal output level (info and above).
    #[default]
    Normal,
    /// Verbose output (debug and above).
    Verbose,
    /// Very verbose output (trace level).
    Trace,
}

impl Verbosity {
    /// Convert verbosity to tracing level filter.
    #[must_use]
    pub fn to_level_filter(&self) -> Level {
        match self {
            Self::Quiet => Level::ERROR,
            Self::Normal => Level::INFO,
            Self::Verbose => Level::DEBUG,
            Self::Trace => Level::TRACE,
        }
    }
}

/// Initialize the logging system.
///
/// This should be called once at application startup. The logging level can be
/// controlled via:
/// 1. The `verbosity` parameter
/// 2. The `RUST_LOG` environment variable (takes precedence)
///
/// # Examples
///
/// ```no_run
/// use flightrecorder::{init_logging, logging::Verbosity};
///
/// // Normal verbosity
/// init_logging(Verbosity::Normal);
///
/// // Verbose output
/// init_logging(Verbosity::Verbose);
/// ```
pub fn init_logging(verbosity: Verbosity) {
    // Build the default filter based on verbosity
    let default_filter = format!("flightrecorder={}", verbosity.to_level_filter());

    // Allow RUST_LOG to override
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&default_filter));

    // Configure the subscriber
    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_file(false)
                .with_line_number(false),
        );

    // Install the subscriber (ignore error if already set)
    let _ = subscriber.try_init();
}

/// Initialize logging for tests.
///
/// This sets up a minimal logging configuration suitable for tests.
/// It only logs warnings and errors by default to keep test output clean.
#[cfg(test)]
pub fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verbosity_to_level() {
        assert_eq!(Verbosity::Quiet.to_level_filter(), Level::ERROR);
        assert_eq!(Verbosity::Normal.to_level_filter(), Level::INFO);
        assert_eq!(Verbosity::Verbose.to_level_filter(), Level::DEBUG);
        assert_eq!(Verbosity::Trace.to_level_filter(), Level::TRACE);
    }

    #[test]
    fn test_verbosity_default() {
        assert_eq!(Verbosity::default(), Verbosity::Normal);
    }

    #[test]
    fn test_verbosity_clone() {
        let v1 = Verbosity::Verbose;
        let v2 = v1;
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_verbosity_debug() {
        let v = Verbosity::Trace;
        let debug_str = format!("{:?}", v);
        assert_eq!(debug_str, "Trace");
    }

    #[test]
    fn test_init_logging_does_not_panic() {
        // This test verifies that init_logging can be called without panicking.
        // Note: The subscriber may already be set from a previous test, which is fine.
        // The function handles this gracefully by ignoring the error.
        init_logging(Verbosity::Normal);
        // If we get here without panicking, the test passes
    }

    #[test]
    fn test_init_logging_with_all_verbosity_levels() {
        // Test that init_logging handles all verbosity levels without panicking
        // Note: Only the first call actually initializes the subscriber
        init_logging(Verbosity::Quiet);
        init_logging(Verbosity::Normal);
        init_logging(Verbosity::Verbose);
        init_logging(Verbosity::Trace);
    }

    #[test]
    fn test_init_test_logging_does_not_panic() {
        init_test_logging();
        // If we get here without panicking, the test passes
    }
}
