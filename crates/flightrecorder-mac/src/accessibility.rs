//! Accessibility-based text field capture for macOS.
//!
//! This module provides functionality to capture text from focused text fields
//! using the macOS Accessibility API. It periodically snapshots the content
//! of the currently focused text field.
//!
//! Note: Full text field capture requires accessibility permissions and uses
//! `AppleScript` for cross-application text access. This provides a compatible
//! approach that works across macOS versions.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, trace, warn};

use crate::permissions;

/// Errors that can occur during accessibility text capture.
#[derive(Debug, Error)]
pub enum AccessibilityError {
    /// Accessibility permission is not granted.
    #[error("accessibility permission not granted: {0}")]
    PermissionDenied(String),

    /// Failed to get the focused element.
    #[error("failed to get focused element: {0}")]
    FocusedElementError(String),

    /// The monitor is not running.
    #[error("accessibility monitor is not running")]
    NotRunning,

    /// Failed to send capture through channel.
    #[error("failed to send capture: channel closed")]
    ChannelClosed,
}

/// Result type for accessibility operations.
pub type Result<T> = std::result::Result<T, AccessibilityError>;

/// A captured text field entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFieldCapture {
    /// The text content from the text field.
    pub content: String,

    /// Hash of the content for deduplication.
    pub content_hash: String,

    /// Timestamp when the capture was detected.
    pub timestamp: DateTime<Utc>,

    /// Source application name.
    pub source_app: Option<String>,

    /// Whether this is from a password field.
    pub is_password_field: bool,
}

impl TextFieldCapture {
    /// Create a new text field capture.
    #[must_use]
    pub fn new(content: String, source_app: Option<String>, is_password_field: bool) -> Self {
        let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        Self {
            content,
            content_hash,
            timestamp: Utc::now(),
            source_app,
            is_password_field,
        }
    }
}

/// Configuration for the accessibility text field monitor.
#[derive(Debug, Clone)]
pub struct AccessibilityMonitorConfig {
    /// Interval between text field snapshots.
    pub snapshot_interval: Duration,

    /// Whether to skip password fields.
    pub skip_password_fields: bool,

    /// Minimum content length to capture.
    pub min_content_length: usize,

    /// Maximum content length to capture.
    pub max_content_length: usize,
}

impl Default for AccessibilityMonitorConfig {
    fn default() -> Self {
        Self {
            snapshot_interval: Duration::from_secs(2),
            skip_password_fields: true,
            min_content_length: 1,
            max_content_length: 1_000_000,
        }
    }
}

/// Monitors focused text fields for content changes.
///
/// The monitor periodically snapshots the focused text field content
/// and sends new captures through a channel.
#[derive(Debug)]
pub struct AccessibilityMonitor {
    config: AccessibilityMonitorConfig,
    running: Arc<AtomicBool>,
    last_hash: Option<String>,
}

impl AccessibilityMonitor {
    /// Create a new accessibility monitor with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AccessibilityMonitorConfig::default())
    }

    /// Create a new accessibility monitor with custom configuration.
    #[must_use]
    pub fn with_config(config: AccessibilityMonitorConfig) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            last_hash: None,
        }
    }

    /// Check if the monitor is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Check if accessibility permissions are enabled.
    #[must_use]
    pub fn has_permission(&self) -> bool {
        permissions::is_accessibility_enabled()
    }

    /// Get the focused text field content if available.
    ///
    /// Uses `AppleScript` to query the focused text element in the frontmost
    /// application. This approach works across macOS versions and most
    /// applications.
    ///
    /// Returns `None` if:
    /// - No text field is focused
    /// - The focused element is not a text field
    /// - `AppleScript` execution fails
    ///
    /// # Errors
    ///
    /// Returns an error if accessibility permission is not granted.
    pub fn get_focused_text(&self) -> Result<Option<FocusedTextField>> {
        if !self.has_permission() {
            return Err(AccessibilityError::PermissionDenied(
                permissions::get_permission_instructions().to_string(),
            ));
        }

        // Use AppleScript to get the focused text field content
        // This requires accessibility permissions
        let script = r#"
            tell application "System Events"
                set frontApp to first process whose frontmost is true
                tell frontApp
                    try
                        set focusedElement to focused of (first UI element whose focused is true)
                        set elementValue to value of (first UI element whose focused is true)
                        if elementValue is not missing value then
                            return elementValue
                        end if
                    end try
                end tell
            end tell
            return ""
        "#;

        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| AccessibilityError::FocusedElementError(e.to_string()))?;

        if !output.status.success() {
            trace!("AppleScript returned non-zero exit code");
            return Ok(None);
        }

        let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if content.is_empty() {
            return Ok(None);
        }

        // Get the source application
        let source_app = get_frontmost_app_name();

        Ok(Some(FocusedTextField {
            content,
            source_app,
            is_password: false, // We can't easily detect password fields via AppleScript
        }))
    }

    /// Check for new text field content.
    ///
    /// Returns `Some(TextFieldCapture)` if new content is detected,
    /// `None` if content hasn't changed.
    ///
    /// # Errors
    ///
    /// Returns an error if accessibility permission is not granted.
    pub fn check_for_changes(&mut self) -> Result<Option<TextFieldCapture>> {
        let Some(field) = self.get_focused_text()? else {
            return Ok(None);
        };

        // Skip password fields if configured
        if field.is_password && self.config.skip_password_fields {
            trace!("Skipping password field");
            return Ok(None);
        }

        // Check content length limits
        if field.content.len() < self.config.min_content_length {
            return Ok(None);
        }

        // Truncate if necessary
        let content = if field.content.len() > self.config.max_content_length {
            field.content[..self.config.max_content_length].to_string()
        } else {
            field.content
        };

        // Hash the content
        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();

        // Check if content has changed
        if self.last_hash.as_ref() == Some(&hash) {
            return Ok(None);
        }

        // Content has changed
        debug!(
            hash = %hash,
            len = content.len(),
            app = ?field.source_app,
            "New text field content detected"
        );
        self.last_hash = Some(hash);

        Ok(Some(TextFieldCapture::new(
            content,
            field.source_app,
            field.is_password,
        )))
    }

    /// Start monitoring text fields and send captures through the channel.
    ///
    /// This runs until `stop()` is called or the receiver is dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if accessibility permission is not granted.
    pub async fn start(&mut self, tx: mpsc::Sender<TextFieldCapture>) -> Result<()> {
        if !self.has_permission() {
            return Err(AccessibilityError::PermissionDenied(
                permissions::get_permission_instructions().to_string(),
            ));
        }

        if self.running.swap(true, Ordering::SeqCst) {
            warn!("Accessibility monitor already running");
            return Ok(());
        }

        debug!(
            interval_secs = self.config.snapshot_interval.as_secs(),
            "Starting accessibility monitor"
        );

        let mut ticker = interval(self.config.snapshot_interval);

        while self.running.load(Ordering::SeqCst) {
            ticker.tick().await;

            match self.check_for_changes() {
                Ok(Some(capture)) => {
                    if tx.send(capture).await.is_err() {
                        debug!("Capture channel closed, stopping monitor");
                        break;
                    }
                }
                Ok(None) => {}
                Err(AccessibilityError::PermissionDenied(msg)) => {
                    warn!(message = %msg, "Accessibility permission denied, stopping monitor");
                    break;
                }
                Err(e) => {
                    warn!(error = %e, "Error checking text field");
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        debug!("Accessibility monitor stopped");
        Ok(())
    }

    /// Stop the accessibility monitor.
    pub fn stop(&self) {
        debug!("Stopping accessibility monitor");
        self.running.store(false, Ordering::SeqCst);
    }

    /// Get a handle that can be used to stop the monitor from another task.
    #[must_use]
    pub fn stop_handle(&self) -> AccessibilityMonitorHandle {
        AccessibilityMonitorHandle {
            running: Arc::clone(&self.running),
        }
    }
}

impl Default for AccessibilityMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle to control a running accessibility monitor.
#[derive(Debug, Clone)]
pub struct AccessibilityMonitorHandle {
    running: Arc<AtomicBool>,
}

impl AccessibilityMonitorHandle {
    /// Stop the associated accessibility monitor.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if the monitor is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Information about the currently focused text field.
#[derive(Debug, Clone)]
pub struct FocusedTextField {
    /// The text content.
    pub content: String,

    /// The source application name.
    pub source_app: Option<String>,

    /// Whether this is a password field.
    pub is_password: bool,
}

/// Get the name of the frontmost application.
fn get_frontmost_app_name() -> Option<String> {
    let output = Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events" to get name of first process whose frontmost is true"#,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_field_capture_new() {
        let capture = TextFieldCapture::new(
            "test content".to_string(),
            Some("TestApp".to_string()),
            false,
        );

        assert_eq!(capture.content, "test content");
        assert!(!capture.content_hash.is_empty());
        assert_eq!(capture.source_app, Some("TestApp".to_string()));
        assert!(!capture.is_password_field);
    }

    #[test]
    fn test_text_field_capture_password() {
        let capture = TextFieldCapture::new("secret".to_string(), None, true);

        assert!(capture.is_password_field);
    }

    #[test]
    fn test_text_field_capture_hash_consistency() {
        let capture1 = TextFieldCapture::new("same content".to_string(), None, false);
        let capture2 = TextFieldCapture::new("same content".to_string(), None, false);

        assert_eq!(capture1.content_hash, capture2.content_hash);
    }

    #[test]
    fn test_accessibility_monitor_config_default() {
        let config = AccessibilityMonitorConfig::default();

        assert_eq!(config.snapshot_interval, Duration::from_secs(2));
        assert!(config.skip_password_fields);
        assert_eq!(config.min_content_length, 1);
        assert_eq!(config.max_content_length, 1_000_000);
    }

    #[test]
    fn test_accessibility_monitor_new() {
        let monitor = AccessibilityMonitor::new();

        assert!(!monitor.is_running());
        assert!(monitor.last_hash.is_none());
    }

    #[test]
    fn test_accessibility_monitor_with_config() {
        let config = AccessibilityMonitorConfig {
            snapshot_interval: Duration::from_secs(5),
            skip_password_fields: false,
            min_content_length: 10,
            max_content_length: 100,
        };
        let monitor = AccessibilityMonitor::with_config(config);

        assert_eq!(monitor.config.snapshot_interval, Duration::from_secs(5));
        assert!(!monitor.config.skip_password_fields);
    }

    #[test]
    fn test_accessibility_monitor_default() {
        let monitor = AccessibilityMonitor::default();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_accessibility_monitor_stop_handle() {
        let monitor = AccessibilityMonitor::new();
        let handle = monitor.stop_handle();

        assert!(!handle.is_running());
    }

    #[test]
    fn test_accessibility_error_display() {
        let error = AccessibilityError::PermissionDenied("test".to_string());
        assert!(error.to_string().contains("permission"));

        let error = AccessibilityError::FocusedElementError("test".to_string());
        assert!(error.to_string().contains("focused"));

        let error = AccessibilityError::NotRunning;
        assert!(error.to_string().contains("not running"));

        let error = AccessibilityError::ChannelClosed;
        assert!(error.to_string().contains("channel"));
    }

    #[test]
    fn test_focused_text_field_debug() {
        let field = FocusedTextField {
            content: "test".to_string(),
            source_app: Some("App".to_string()),
            is_password: false,
        };
        let debug = format!("{field:?}");
        assert!(debug.contains("FocusedTextField"));
    }

    #[test]
    fn test_focused_text_field_clone() {
        let field = FocusedTextField {
            content: "test".to_string(),
            source_app: Some("App".to_string()),
            is_password: true,
        };
        let cloned = field.clone();
        assert_eq!(field.content, cloned.content);
        assert_eq!(field.is_password, cloned.is_password);
    }
}
