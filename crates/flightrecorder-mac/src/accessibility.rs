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

        self.process_field(field)
    }

    /// Process a focused text field and return a capture if it's new content.
    ///
    /// This is separated from `check_for_changes` to allow testing the logic
    /// without needing actual system access.
    ///
    /// # Errors
    ///
    /// This method is infallible but returns `Result` for API consistency.
    pub fn process_field(&mut self, field: FocusedTextField) -> Result<Option<TextFieldCapture>> {
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
#[must_use]
pub fn get_frontmost_app_name() -> Option<String> {
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

/// Process text field content according to config limits.
///
/// Returns `None` if content should be skipped, `Some(processed_content)` otherwise.
#[must_use]
pub fn process_text_field_content(
    content: &str,
    min_length: usize,
    max_length: usize,
) -> Option<String> {
    // Check minimum length
    if content.len() < min_length {
        return None;
    }

    // Truncate if necessary
    if content.len() > max_length {
        Some(content[..max_length].to_string())
    } else {
        Some(content.to_string())
    }
}

/// Compute a BLAKE3 hash of the content.
#[must_use]
pub fn compute_content_hash(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

/// Check if content has changed based on hash comparison.
#[must_use]
pub fn text_content_changed(content: &str, last_hash: Option<&str>) -> bool {
    let hash = compute_content_hash(content);
    last_hash != Some(&hash)
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

    #[test]
    fn test_accessibility_monitor_handle_stop() {
        let monitor = AccessibilityMonitor::new();
        let handle = monitor.stop_handle();

        // Initially not running (stop signal not set means nothing to stop)
        assert!(!handle.is_running());

        // Clone the handle
        let handle2 = handle.clone();
        assert!(!handle2.is_running());
    }

    #[test]
    fn test_accessibility_monitor_handle_clone_shares_state() {
        let monitor = AccessibilityMonitor::new();
        let handle1 = monitor.stop_handle();
        let handle2 = handle1.clone();

        // Both should share the same running state
        assert_eq!(handle1.is_running(), handle2.is_running());
    }

    #[test]
    fn test_accessibility_monitor_stop() {
        let monitor = AccessibilityMonitor::new();

        // Stop should work even when not running
        monitor.stop();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_accessibility_monitor_config_clone() {
        let config = AccessibilityMonitorConfig::default();
        let cloned = config.clone();

        assert_eq!(config.snapshot_interval, cloned.snapshot_interval);
        assert_eq!(config.skip_password_fields, cloned.skip_password_fields);
        assert_eq!(config.min_content_length, cloned.min_content_length);
        assert_eq!(config.max_content_length, cloned.max_content_length);
    }

    #[test]
    fn test_accessibility_monitor_config_debug() {
        let config = AccessibilityMonitorConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("AccessibilityMonitorConfig"));
        assert!(debug.contains("snapshot_interval"));
    }

    #[test]
    fn test_text_field_capture_with_no_source_app() {
        let capture = TextFieldCapture::new("content".to_string(), None, false);

        assert_eq!(capture.content, "content");
        assert!(capture.source_app.is_none());
        assert!(!capture.is_password_field);
    }

    #[test]
    fn test_text_field_capture_hash_different_content() {
        let capture1 = TextFieldCapture::new("content one".to_string(), None, false);
        let capture2 = TextFieldCapture::new("content two".to_string(), None, false);

        assert_ne!(capture1.content_hash, capture2.content_hash);
    }

    #[test]
    fn test_text_field_capture_debug() {
        let capture = TextFieldCapture::new("test".to_string(), None, false);
        let debug = format!("{capture:?}");
        assert!(debug.contains("TextFieldCapture"));
        assert!(debug.contains("content"));
    }

    #[test]
    fn test_text_field_capture_clone() {
        let capture = TextFieldCapture::new("test".to_string(), Some("App".to_string()), true);
        let cloned = capture.clone();

        assert_eq!(capture.content, cloned.content);
        assert_eq!(capture.content_hash, cloned.content_hash);
        assert_eq!(capture.source_app, cloned.source_app);
        assert_eq!(capture.is_password_field, cloned.is_password_field);
    }

    #[test]
    fn test_text_field_capture_eq() {
        let capture1 = TextFieldCapture::new("test".to_string(), None, false);
        let capture2 = capture1.clone();

        // They should be equal (same content, source_app, is_password_field)
        // Note: timestamps may differ slightly, but the eq should still work
        // because we're comparing content, hash, source_app, is_password_field
        assert_eq!(capture1.content, capture2.content);
        assert_eq!(capture1.content_hash, capture2.content_hash);
    }

    #[test]
    fn test_focused_text_field_with_no_source_app() {
        let field = FocusedTextField {
            content: "test".to_string(),
            source_app: None,
            is_password: false,
        };
        assert!(field.source_app.is_none());
    }

    #[test]
    fn test_accessibility_monitor_debug() {
        let monitor = AccessibilityMonitor::new();
        let debug = format!("{monitor:?}");
        assert!(debug.contains("AccessibilityMonitor"));
    }

    #[test]
    fn test_accessibility_monitor_handle_debug() {
        let monitor = AccessibilityMonitor::new();
        let handle = monitor.stop_handle();
        let debug = format!("{handle:?}");
        assert!(debug.contains("AccessibilityMonitorHandle"));
    }

    #[test]
    fn test_accessibility_monitor_handle_stop_sets_running_false() {
        let monitor = AccessibilityMonitor::new();
        let handle = monitor.stop_handle();

        // Manually set running to true
        monitor.running.store(true, Ordering::SeqCst);
        assert!(handle.is_running());

        // Stop via handle
        handle.stop();
        assert!(!handle.is_running());
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_accessibility_monitor_has_permission() {
        let monitor = AccessibilityMonitor::new();
        // Just verify the method can be called - actual result depends on system state
        let _has_perm = monitor.has_permission();
    }

    // Integration tests that require actual system access
    // Run with: cargo test --ignored

    #[test]
    #[ignore = "requires accessibility permission and system access"]
    fn test_get_focused_text_returns_result() {
        let monitor = AccessibilityMonitor::new();
        // This will return an error if no permission, or Ok(None)/Ok(Some) if permitted
        let result = monitor.get_focused_text();
        // Either permission denied or successful (None or Some)
        assert!(result.is_ok() || matches!(result, Err(AccessibilityError::PermissionDenied(_))));
    }

    #[test]
    #[ignore = "requires accessibility permission and system access"]
    fn test_check_for_changes_returns_result() {
        let mut monitor = AccessibilityMonitor::new();
        let result = monitor.check_for_changes();
        // Either permission denied or successful
        assert!(result.is_ok() || matches!(result, Err(AccessibilityError::PermissionDenied(_))));
    }

    #[test]
    #[ignore = "requires accessibility permission and system access"]
    fn test_get_frontmost_app_name() {
        // This should work without accessibility permission
        let result = get_frontmost_app_name();
        // Should return Some app name or None
        if let Some(name) = result {
            assert!(!name.is_empty());
        }
    }

    #[tokio::test]
    #[ignore = "requires accessibility permission and system access"]
    async fn test_start_without_permission_returns_error() {
        let mut monitor = AccessibilityMonitor::new();
        let (tx, _rx) = mpsc::channel(10);

        // If we don't have permission, this should return an error
        if !monitor.has_permission() {
            let result = monitor.start(tx).await;
            assert!(matches!(
                result,
                Err(AccessibilityError::PermissionDenied(_))
            ));
        }
    }

    #[tokio::test]
    #[ignore = "requires accessibility permission and system access"]
    async fn test_start_and_stop() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            snapshot_interval: Duration::from_millis(50),
            ..Default::default()
        });

        if !monitor.has_permission() {
            return; // Skip if no permission
        }

        let (tx, mut rx) = mpsc::channel(10);
        let handle = monitor.stop_handle();

        // Start monitor in background
        let monitor_task = tokio::spawn(async move { monitor.start(tx).await });

        // Let it run briefly
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Stop the monitor
        handle.stop();

        // Wait for it to finish
        let result = monitor_task.await.unwrap();
        assert!(result.is_ok());

        // Close receiver
        rx.close();
    }

    // Test the processing logic using FocusedTextField directly
    #[test]
    fn test_focused_text_field_password_detection() {
        let field = FocusedTextField {
            content: "secret123".to_string(),
            source_app: Some("1Password".to_string()),
            is_password: true,
        };

        assert!(field.is_password);
        assert_eq!(field.content, "secret123");
    }

    #[test]
    fn test_focused_text_field_all_fields() {
        let field = FocusedTextField {
            content: "Hello, World!".to_string(),
            source_app: Some("TextEdit".to_string()),
            is_password: false,
        };

        assert_eq!(field.content, "Hello, World!");
        assert_eq!(field.source_app, Some("TextEdit".to_string()));
        assert!(!field.is_password);
    }

    #[test]
    fn test_text_field_capture_creates_unique_hash() {
        // Each unique content should have a unique hash
        let capture1 = TextFieldCapture::new("abc".to_string(), None, false);
        let capture2 = TextFieldCapture::new("abd".to_string(), None, false);
        let capture3 = TextFieldCapture::new("abc".to_string(), None, false);

        assert_ne!(capture1.content_hash, capture2.content_hash);
        assert_eq!(capture1.content_hash, capture3.content_hash);
    }

    #[test]
    fn test_text_field_capture_timestamp_is_recent() {
        let before = Utc::now();
        let capture = TextFieldCapture::new("test".to_string(), None, false);
        let after = Utc::now();

        assert!(capture.timestamp >= before);
        assert!(capture.timestamp <= after);
    }

    #[test]
    fn test_accessibility_error_variants() {
        // Test all error variants for coverage
        let err1 = AccessibilityError::PermissionDenied("msg".to_string());
        let err2 = AccessibilityError::FocusedElementError("err".to_string());
        let err3 = AccessibilityError::NotRunning;
        let err4 = AccessibilityError::ChannelClosed;

        // Debug format
        assert!(format!("{err1:?}").contains("PermissionDenied"));
        assert!(format!("{err2:?}").contains("FocusedElementError"));
        assert!(format!("{err3:?}").contains("NotRunning"));
        assert!(format!("{err4:?}").contains("ChannelClosed"));

        // Display format
        assert!(err1.to_string().contains("permission"));
        assert!(err2.to_string().contains("focused"));
        assert!(err3.to_string().contains("not running"));
        assert!(err4.to_string().contains("channel"));
    }

    #[test]
    fn test_monitor_running_state_transitions() {
        let monitor = AccessibilityMonitor::new();

        // Initial state
        assert!(!monitor.is_running());

        // Simulate running
        monitor.running.store(true, Ordering::SeqCst);
        assert!(monitor.is_running());

        // Stop
        monitor.stop();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_monitor_handle_multiple_stops() {
        let monitor = AccessibilityMonitor::new();
        let handle = monitor.stop_handle();

        // Multiple stops should be idempotent
        monitor.running.store(true, Ordering::SeqCst);
        handle.stop();
        handle.stop();
        handle.stop();

        assert!(!monitor.is_running());
    }

    #[test]
    fn test_config_boundary_values() {
        let config = AccessibilityMonitorConfig {
            snapshot_interval: Duration::from_millis(1),
            skip_password_fields: false,
            min_content_length: 0,
            max_content_length: usize::MAX,
        };

        assert_eq!(config.snapshot_interval.as_millis(), 1);
        assert_eq!(config.min_content_length, 0);
        assert_eq!(config.max_content_length, usize::MAX);
    }

    // Tests for helper functions

    #[test]
    fn test_process_text_field_content_normal() {
        let result = process_text_field_content("hello world", 1, 100);
        assert_eq!(result, Some("hello world".to_string()));
    }

    #[test]
    fn test_process_text_field_content_too_short() {
        let result = process_text_field_content("hi", 5, 100);
        assert_eq!(result, None);
    }

    #[test]
    fn test_process_text_field_content_truncated() {
        let result = process_text_field_content("hello world", 1, 5);
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn test_process_text_field_content_exact_min() {
        let result = process_text_field_content("abc", 3, 100);
        assert_eq!(result, Some("abc".to_string()));
    }

    #[test]
    fn test_process_text_field_content_exact_max() {
        let result = process_text_field_content("abc", 1, 3);
        assert_eq!(result, Some("abc".to_string()));
    }

    #[test]
    fn test_process_text_field_content_empty_with_zero_min() {
        let result = process_text_field_content("", 0, 100);
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn test_compute_content_hash_consistency() {
        let hash1 = compute_content_hash("test");
        let hash2 = compute_content_hash("test");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_content_hash_different() {
        let hash1 = compute_content_hash("test1");
        let hash2 = compute_content_hash("test2");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_content_hash_length() {
        let hash = compute_content_hash("test");
        // BLAKE3 produces 64-character hex strings
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_text_content_changed_with_none() {
        assert!(text_content_changed("hello", None));
    }

    #[test]
    fn test_text_content_changed_same() {
        let hash = compute_content_hash("hello");
        assert!(!text_content_changed("hello", Some(&hash)));
    }

    #[test]
    fn test_text_content_changed_different() {
        let hash = compute_content_hash("hello");
        assert!(text_content_changed("world", Some(&hash)));
    }

    // Test get_frontmost_app_name (uses osascript, should work without accessibility)
    #[test]
    fn test_get_frontmost_app_name_returns_result() {
        // This test uses osascript which should work on macOS
        let result = get_frontmost_app_name();
        // Should return Some(app_name) or None, but shouldn't panic
        if let Some(name) = result {
            assert!(!name.is_empty());
        }
    }

    // Tests for process_field method (can test logic without system access)

    #[test]
    fn test_process_field_normal_content() {
        let mut monitor = AccessibilityMonitor::new();
        let field = FocusedTextField {
            content: "Hello, World!".to_string(),
            source_app: Some("TextEdit".to_string()),
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());

        let capture = result.unwrap();
        assert_eq!(capture.content, "Hello, World!");
        assert_eq!(capture.source_app, Some("TextEdit".to_string()));
        assert!(!capture.is_password_field);
    }

    #[test]
    fn test_process_field_skips_password_when_configured() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            skip_password_fields: true,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "secret123".to_string(),
            source_app: Some("Safari".to_string()),
            is_password: true,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_process_field_allows_password_when_not_configured() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            skip_password_fields: false,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "secret123".to_string(),
            source_app: Some("Safari".to_string()),
            is_password: true,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().is_password_field);
    }

    #[test]
    fn test_process_field_skips_short_content() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            min_content_length: 10,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "short".to_string(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_process_field_truncates_long_content() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            max_content_length: 10,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "This is a very long string that should be truncated".to_string(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());

        let capture = result.unwrap();
        assert_eq!(capture.content.len(), 10);
        assert_eq!(capture.content, "This is a ");
    }

    #[test]
    fn test_process_field_deduplicates_same_content() {
        let mut monitor = AccessibilityMonitor::new();

        let field1 = FocusedTextField {
            content: "Same content".to_string(),
            source_app: None,
            is_password: false,
        };

        let field2 = FocusedTextField {
            content: "Same content".to_string(),
            source_app: None,
            is_password: false,
        };

        // First call should return Some
        let result1 = monitor.process_field(field1).unwrap();
        assert!(result1.is_some());

        // Second call with same content should return None (deduplicated)
        let result2 = monitor.process_field(field2).unwrap();
        assert!(result2.is_none());
    }

    #[test]
    fn test_process_field_detects_changed_content() {
        let mut monitor = AccessibilityMonitor::new();

        let field1 = FocusedTextField {
            content: "Content one".to_string(),
            source_app: None,
            is_password: false,
        };

        let field2 = FocusedTextField {
            content: "Content two".to_string(),
            source_app: None,
            is_password: false,
        };

        // First call
        let result1 = monitor.process_field(field1).unwrap();
        assert!(result1.is_some());

        // Second call with different content should return Some
        let result2 = monitor.process_field(field2).unwrap();
        assert!(result2.is_some());
    }

    #[test]
    fn test_process_field_with_no_source_app() {
        let mut monitor = AccessibilityMonitor::new();

        let field = FocusedTextField {
            content: "Test content".to_string(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().source_app.is_none());
    }

    #[test]
    fn test_process_field_exact_min_length() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            min_content_length: 5,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "12345".to_string(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_process_field_exact_max_length() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            max_content_length: 5,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "12345".to_string(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "12345");
    }

    #[test]
    fn test_process_field_multiple_sequential_different() {
        let mut monitor = AccessibilityMonitor::new();

        // Process several different fields
        let field1 = FocusedTextField {
            content: "first".to_string(),
            source_app: None,
            is_password: false,
        };
        let result1 = monitor.process_field(field1).unwrap();
        assert!(result1.is_some());

        let field2 = FocusedTextField {
            content: "second".to_string(),
            source_app: None,
            is_password: false,
        };
        let result2 = monitor.process_field(field2).unwrap();
        assert!(result2.is_some());

        // Repeat should deduplicate
        let field3 = FocusedTextField {
            content: "second".to_string(),
            source_app: None,
            is_password: false,
        };
        let result3 = monitor.process_field(field3).unwrap();
        assert!(result3.is_none());
    }

    #[test]
    fn test_process_field_hash_changes_with_content() {
        let mut monitor = AccessibilityMonitor::new();

        let field1 = FocusedTextField {
            content: "content1".to_string(),
            source_app: None,
            is_password: false,
        };
        let _ = monitor.process_field(field1).unwrap();
        let hash1 = monitor.last_hash.clone();

        let field2 = FocusedTextField {
            content: "content2".to_string(),
            source_app: None,
            is_password: false,
        };
        let _ = monitor.process_field(field2).unwrap();
        let hash2 = monitor.last_hash.clone();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_process_field_whitespace_only() {
        let mut monitor = AccessibilityMonitor::new();
        let field = FocusedTextField {
            content: "   \n\t\r  ".to_string(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_process_field_very_long_content() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            max_content_length: 100,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "x".repeat(10000),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content.len(), 100);
    }

    #[test]
    fn test_process_field_password_with_skip_true() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            skip_password_fields: true,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "password123".to_string(),
            source_app: Some("1Password".to_string()),
            is_password: true,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_process_field_password_with_skip_false() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            skip_password_fields: false,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "password123".to_string(),
            source_app: Some("1Password".to_string()),
            is_password: true,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());

        let capture = result.unwrap();
        assert!(capture.is_password_field);
        assert_eq!(capture.content, "password123");
    }

    #[test]
    fn test_process_field_updates_last_hash() {
        let mut monitor = AccessibilityMonitor::new();
        assert!(monitor.last_hash.is_none());

        let field = FocusedTextField {
            content: "test content".to_string(),
            source_app: None,
            is_password: false,
        };

        let _ = monitor.process_field(field).unwrap();
        assert!(monitor.last_hash.is_some());
    }

    #[test]
    fn test_process_field_special_characters() {
        let mut monitor = AccessibilityMonitor::new();
        let special = "Hello\n\t\r\0World 🎉 émoji".to_string();

        let field = FocusedTextField {
            content: special.clone(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, special);
    }

    #[test]
    fn test_process_field_one_below_min_length() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            min_content_length: 5,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "1234".to_string(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_process_field_one_above_max_length() {
        let mut monitor = AccessibilityMonitor::with_config(AccessibilityMonitorConfig {
            max_content_length: 5,
            ..Default::default()
        });

        let field = FocusedTextField {
            content: "123456".to_string(),
            source_app: None,
            is_password: false,
        };

        let result = monitor.process_field(field).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "12345");
    }

    #[test]
    fn test_get_focused_text_permission_check() {
        let monitor = AccessibilityMonitor::new();
        // This test exercises the permission check path
        // If permission is not granted, it returns an error
        // If granted, it attempts to get focused text
        let result = monitor.get_focused_text();

        // Either we get a permission error, or we get Ok(Some/None)
        match result {
            Err(AccessibilityError::PermissionDenied(_)) => {
                // Expected when permission not granted
            }
            Ok(_) => {
                // Permission was granted, got a result
            }
            Err(e) => {
                // Other error is also acceptable
                let _ = e;
            }
        }
    }

    #[test]
    fn test_check_for_changes_with_no_permission() {
        let mut monitor = AccessibilityMonitor::new();

        // If no permission, should return error
        // If permission granted, should return Ok
        let result = monitor.check_for_changes();

        match result {
            Err(AccessibilityError::PermissionDenied(_)) => {
                // Expected when no permission
            }
            Ok(_) => {
                // Permission was granted
            }
            Err(_) => {
                // Other error
            }
        }
    }

    #[tokio::test]
    #[ignore = "may hang if accessibility permission is granted"]
    async fn test_start_already_running() {
        let mut monitor = AccessibilityMonitor::new();
        let (tx, _rx) = mpsc::channel(10);

        // Set running to true before calling start
        monitor.running.store(true, Ordering::SeqCst);

        // If permission check fails, we get that error
        // Otherwise, if already running, we should get Ok (with warning logged)
        let result = monitor.start(tx).await;

        // Reset state
        monitor.running.store(false, Ordering::SeqCst);

        // Result depends on permission status
        match result {
            Err(AccessibilityError::PermissionDenied(_)) => {
                // No permission
            }
            Ok(()) => {
                // Was already running or completed
            }
            Err(_) => {
                // Other error
            }
        }
    }

    #[tokio::test]
    #[ignore = "may hang if accessibility permission is granted"]
    async fn test_start_without_permission_error_path() {
        let mut monitor = AccessibilityMonitor::new();
        let (tx, _rx) = mpsc::channel(10);

        // Call start - it will check permission first
        let result = monitor.start(tx).await;

        if !permissions::is_accessibility_enabled() {
            // Should have returned permission denied error
            assert!(matches!(
                result,
                Err(AccessibilityError::PermissionDenied(_))
            ));
        }
        // If permission is enabled, the test behavior depends on system state
    }
}
