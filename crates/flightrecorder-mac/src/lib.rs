//! macOS-specific implementation for flightrecorder.
//!
//! This crate provides macOS-specific functionality for the flightrecorder project,
//! including clipboard monitoring and text capture via accessibility APIs.

#![cfg(target_os = "macos")]
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

pub mod accessibility;
pub mod clipboard;
pub mod permissions;

pub use accessibility::{
    AccessibilityError, AccessibilityMonitor, AccessibilityMonitorConfig,
    AccessibilityMonitorHandle, FocusedTextField, TextFieldCapture,
};
pub use clipboard::{
    ClipboardCapture, ClipboardError, ClipboardMonitor, ClipboardMonitorConfig,
    ClipboardMonitorHandle,
};
pub use permissions::{
    check_permission, get_permission_instructions, is_accessibility_enabled,
    request_accessibility_permission, PermissionError, PermissionStatus,
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
    fn test_clipboard_exports() {
        // Verify that public clipboard types are accessible
        let _ = ClipboardMonitorConfig::default();
        let monitor = ClipboardMonitor::new();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_accessibility_exports() {
        // Verify that public accessibility types are accessible
        let _ = AccessibilityMonitorConfig::default();
        let monitor = AccessibilityMonitor::new();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_permission_exports() {
        // Verify that permission functions are accessible
        let status = check_permission();
        // Status should have a valid description
        assert!(!status.description.is_empty());
    }
}
