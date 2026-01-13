//! Permission handling for macOS accessibility features.
//!
//! This module provides utilities for checking and requesting accessibility
//! permissions on macOS. Accessibility permissions are required to capture
//! text from focused text fields across applications.

use macos_accessibility_client::accessibility;
use thiserror::Error;

/// Errors related to accessibility permissions.
#[derive(Debug, Error)]
pub enum PermissionError {
    /// Accessibility permission is not granted.
    #[error("accessibility permission not granted")]
    NotGranted,

    /// Failed to check permissions.
    #[error("failed to check accessibility permissions: {0}")]
    CheckFailed(String),
}

/// Result type for permission operations.
pub type Result<T> = std::result::Result<T, PermissionError>;

/// Information about the current accessibility permission status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionStatus {
    /// Whether accessibility permission is currently granted.
    pub is_granted: bool,

    /// Human-readable description of the status.
    pub description: String,
}

impl PermissionStatus {
    /// Create a new granted status.
    #[must_use]
    pub fn granted() -> Self {
        Self {
            is_granted: true,
            description: "Accessibility permission is granted".to_string(),
        }
    }

    /// Create a new not-granted status.
    #[must_use]
    pub fn not_granted() -> Self {
        Self {
            is_granted: false,
            description: "Accessibility permission is not granted".to_string(),
        }
    }
}

/// Check if the application has accessibility permissions.
///
/// On macOS, accessibility permissions are required to:
/// - Read text from focused text fields in other applications
/// - Get information about the frontmost application's UI elements
///
/// # Returns
///
/// Returns `true` if accessibility permissions are granted, `false` otherwise.
#[must_use]
pub fn is_accessibility_enabled() -> bool {
    accessibility::application_is_trusted()
}

/// Check accessibility permission status.
///
/// Returns detailed information about whether accessibility permissions
/// are granted.
#[must_use]
pub fn check_permission() -> PermissionStatus {
    if is_accessibility_enabled() {
        PermissionStatus::granted()
    } else {
        PermissionStatus::not_granted()
    }
}

/// Prompt the user to grant accessibility permissions.
///
/// This opens the System Preferences to the Privacy & Security > Accessibility
/// pane, where the user can grant permissions to the application.
///
/// Note: The user must manually enable the permission. This function just
/// opens the settings pane.
///
/// # Returns
///
/// Returns `true` if the system preferences were successfully opened.
#[must_use]
pub fn request_accessibility_permission() -> bool {
    accessibility::application_is_trusted_with_prompt()
}

/// Get instructions for how to grant accessibility permissions.
///
/// Returns a human-readable string with instructions for the user.
#[must_use]
pub fn get_permission_instructions() -> &'static str {
    r"To enable accessibility features:

1. Open System Preferences (or System Settings on macOS Ventura+)
2. Go to Privacy & Security > Accessibility
3. Click the lock icon to make changes (you may need to enter your password)
4. Find 'fliterec' in the list and enable it
5. If 'fliterec' is not listed, click the '+' button and add it

After granting permission, restart the fliterec daemon."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_status_granted() {
        let status = PermissionStatus::granted();
        assert!(status.is_granted);
        assert!(!status.description.is_empty());
    }

    #[test]
    fn test_permission_status_not_granted() {
        let status = PermissionStatus::not_granted();
        assert!(!status.is_granted);
        assert!(!status.description.is_empty());
    }

    #[test]
    fn test_permission_status_debug() {
        let status = PermissionStatus::granted();
        let debug = format!("{status:?}");
        assert!(debug.contains("PermissionStatus"));
    }

    #[test]
    fn test_permission_status_clone() {
        let status = PermissionStatus::granted();
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_get_permission_instructions() {
        let instructions = get_permission_instructions();
        assert!(instructions.contains("Privacy"));
        assert!(instructions.contains("Accessibility"));
    }

    #[test]
    fn test_check_permission_returns_valid_status() {
        // This test doesn't verify the actual permission state,
        // just that the function works without panicking
        let status = check_permission();
        // The status should have a description regardless of permission state
        assert!(!status.description.is_empty());
    }

    #[test]
    fn test_permission_error_display() {
        let error = PermissionError::NotGranted;
        assert_eq!(error.to_string(), "accessibility permission not granted");

        let error = PermissionError::CheckFailed("test error".to_string());
        assert!(error.to_string().contains("test error"));
    }
}
