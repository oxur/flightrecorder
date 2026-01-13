//! macOS-specific implementation for flightrecorder.
//!
//! This crate provides macOS-specific functionality for the flightrecorder project,
//! including clipboard monitoring and text capture via accessibility APIs.

#![cfg(target_os = "macos")]
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

pub mod clipboard;

pub use clipboard::{
    ClipboardCapture, ClipboardError, ClipboardMonitor, ClipboardMonitorConfig,
    ClipboardMonitorHandle,
};

/// Initialize macOS-specific components.
///
/// This performs any necessary setup for the macOS platform,
/// such as checking for required permissions.
///
/// # Errors
///
/// Returns an error if initialization fails.
pub fn init() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Initializing macOS platform components");
    Ok(())
}

/// Get the platform name.
#[must_use]
pub fn platform_name() -> &'static str {
    "macOS"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        assert!(init().is_ok());
    }

    #[test]
    fn test_platform_name() {
        assert_eq!(platform_name(), "macOS");
    }

    #[test]
    fn test_exports() {
        // Verify that public types are accessible
        let _ = ClipboardMonitorConfig::default();
        let monitor = ClipboardMonitor::new();
        assert!(!monitor.is_running());
    }
}
