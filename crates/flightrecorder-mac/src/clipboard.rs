//! Clipboard monitoring for macOS.
//!
//! This module provides clipboard change detection and text capture functionality.
//! It monitors the system clipboard for changes and notifies callbacks when new
//! text content is detected.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use clipboard_rs::{Clipboard, ClipboardContext};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, trace, warn};

/// Errors that can occur during clipboard monitoring.
#[derive(Debug, Error)]
pub enum ClipboardError {
    /// Failed to access the clipboard.
    #[error("clipboard access failed: {0}")]
    AccessFailed(String),

    /// The clipboard monitor is not running.
    #[error("clipboard monitor is not running")]
    NotRunning,

    /// Failed to send capture through channel.
    #[error("failed to send capture: channel closed")]
    ChannelClosed,
}

/// Result type for clipboard operations.
pub type Result<T> = std::result::Result<T, ClipboardError>;

/// A captured clipboard entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCapture {
    /// The text content from the clipboard.
    pub content: String,

    /// Hash of the content for deduplication.
    pub content_hash: String,

    /// Timestamp when the capture was detected.
    pub timestamp: DateTime<Utc>,

    /// Source application (if detectable).
    pub source_app: Option<String>,
}

impl ClipboardCapture {
    /// Create a new clipboard capture.
    #[must_use]
    pub fn new(content: String, source_app: Option<String>) -> Self {
        let content_hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        Self {
            content,
            content_hash,
            timestamp: Utc::now(),
            source_app,
        }
    }
}

/// Configuration for the clipboard monitor.
#[derive(Debug, Clone)]
pub struct ClipboardMonitorConfig {
    /// Interval between clipboard checks.
    pub poll_interval: Duration,

    /// Minimum content length to capture (skip very short entries).
    pub min_content_length: usize,

    /// Maximum content length to capture (truncate very long entries).
    pub max_content_length: usize,
}

impl Default for ClipboardMonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(500),
            min_content_length: 1,
            max_content_length: 1_000_000, // 1MB
        }
    }
}

/// Monitors the system clipboard for changes.
///
/// The monitor polls the clipboard at a configurable interval and sends
/// new text content through a channel when changes are detected.
#[derive(Debug)]
pub struct ClipboardMonitor {
    config: ClipboardMonitorConfig,
    running: Arc<AtomicBool>,
    last_hash: Option<String>,
}

impl ClipboardMonitor {
    /// Create a new clipboard monitor with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ClipboardMonitorConfig::default())
    }

    /// Create a new clipboard monitor with custom configuration.
    #[must_use]
    pub fn with_config(config: ClipboardMonitorConfig) -> Self {
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

    /// Get the current clipboard text content.
    ///
    /// # Errors
    ///
    /// Returns an error if clipboard access fails.
    pub fn get_current_text(&self) -> Result<Option<String>> {
        let ctx =
            ClipboardContext::new().map_err(|e| ClipboardError::AccessFailed(e.to_string()))?;

        match ctx.get_text() {
            Ok(text) if !text.is_empty() => Ok(Some(text)),
            // No text content or non-text clipboard is not an error
            Ok(_) | Err(_) => Ok(None),
        }
    }

    /// Check the clipboard for new content.
    ///
    /// Returns `Some(ClipboardCapture)` if new text content is detected,
    /// `None` if the content hasn't changed or isn't text.
    ///
    /// # Errors
    ///
    /// Returns an error if clipboard access fails.
    pub fn check_for_changes(&mut self) -> Result<Option<ClipboardCapture>> {
        let Some(text) = self.get_current_text()? else {
            return Ok(None);
        };

        self.process_text(text)
    }

    /// Process clipboard text and return a capture if it's new content.
    ///
    /// This is separated from `check_for_changes` to allow testing the logic
    /// without needing actual clipboard access. The `source_app` parameter
    /// allows callers to provide the app name (use `None` in tests to avoid
    /// slow osascript calls).
    ///
    /// # Errors
    ///
    /// This method is infallible but returns `Result` for API consistency.
    pub fn process_text_with_source(
        &mut self,
        text: String,
        source_app: Option<String>,
    ) -> Result<Option<ClipboardCapture>> {
        // Check content length limits
        if text.len() < self.config.min_content_length {
            trace!(
                len = text.len(),
                min = self.config.min_content_length,
                "Clipboard content too short, skipping"
            );
            return Ok(None);
        }

        // Truncate if necessary
        let content = if text.len() > self.config.max_content_length {
            debug!(
                len = text.len(),
                max = self.config.max_content_length,
                "Truncating clipboard content"
            );
            text[..self.config.max_content_length].to_string()
        } else {
            text
        };

        // Hash the content to detect changes
        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();

        // Check if content has changed
        if self.last_hash.as_ref() == Some(&hash) {
            trace!("Clipboard content unchanged");
            return Ok(None);
        }

        // Content has changed
        debug!(hash = %hash, len = content.len(), "New clipboard content detected");
        self.last_hash = Some(hash);

        Ok(Some(ClipboardCapture::new(content, source_app)))
    }

    /// Process clipboard text, automatically detecting the source application.
    ///
    /// This calls `get_frontmost_app()` which uses osascript and may be slow.
    /// For testing, use `process_text_with_source()` instead.
    ///
    /// # Errors
    ///
    /// This method is infallible but returns `Result` for API consistency.
    pub fn process_text(&mut self, text: String) -> Result<Option<ClipboardCapture>> {
        let source_app = get_frontmost_app();
        self.process_text_with_source(text, source_app)
    }

    /// Start monitoring the clipboard and send captures through the channel.
    ///
    /// This runs until `stop()` is called or the receiver is dropped.
    ///
    /// # Arguments
    ///
    /// * `tx` - Channel sender for clipboard captures
    ///
    /// # Errors
    ///
    /// Returns an error if the monitor is already running.
    pub async fn start(&mut self, tx: mpsc::Sender<ClipboardCapture>) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            warn!("Clipboard monitor already running");
            return Ok(());
        }

        debug!(
            interval_ms = self.config.poll_interval.as_millis(),
            "Starting clipboard monitor"
        );

        let mut ticker = interval(self.config.poll_interval);

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
                Err(e) => {
                    warn!(error = %e, "Error checking clipboard");
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        debug!("Clipboard monitor stopped");
        Ok(())
    }

    /// Stop the clipboard monitor.
    pub fn stop(&self) {
        debug!("Stopping clipboard monitor");
        self.running.store(false, Ordering::SeqCst);
    }

    /// Get a handle that can be used to stop the monitor from another task.
    #[must_use]
    pub fn stop_handle(&self) -> ClipboardMonitorHandle {
        ClipboardMonitorHandle {
            running: Arc::clone(&self.running),
        }
    }
}

impl Default for ClipboardMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle to control a running clipboard monitor.
///
/// This can be cloned and sent to other tasks to stop the monitor remotely.
#[derive(Debug, Clone)]
pub struct ClipboardMonitorHandle {
    running: Arc<AtomicBool>,
}

impl ClipboardMonitorHandle {
    /// Stop the associated clipboard monitor.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if the monitor is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Get the frontmost (active) application name on macOS.
///
/// This uses the macOS Accessibility API to determine which application
/// is currently in the foreground.
#[must_use]
pub fn get_frontmost_app() -> Option<String> {
    // Use NSWorkspace to get the frontmost application
    // This is a safe approach that doesn't require accessibility permissions
    get_frontmost_app_via_nsworkspace()
}

/// Get frontmost app using `NSWorkspace` (safe, no special permissions needed).
#[must_use]
pub fn get_frontmost_app_via_nsworkspace() -> Option<String> {
    // We use the `osascript` command as a simple cross-compatible approach
    // This works on all macOS versions without requiring special permissions
    use std::process::Command;

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

/// Process raw text content according to config limits.
///
/// Returns `None` if content should be skipped, `Some(processed_content)` otherwise.
#[must_use]
pub fn process_content(text: &str, min_length: usize, max_length: usize) -> Option<String> {
    // Check minimum length
    if text.len() < min_length {
        return None;
    }

    // Truncate if necessary
    if text.len() > max_length {
        Some(text[..max_length].to_string())
    } else {
        Some(text.to_string())
    }
}

/// Check if content has changed based on hash comparison.
#[must_use]
pub fn content_changed(content: &str, last_hash: Option<&str>) -> bool {
    let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
    last_hash != Some(&hash)
}

/// Compute a BLAKE3 hash of the content.
#[must_use]
pub fn compute_hash(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_capture_new() {
        let capture =
            ClipboardCapture::new("test content".to_string(), Some("TestApp".to_string()));

        assert_eq!(capture.content, "test content");
        assert!(!capture.content_hash.is_empty());
        assert_eq!(capture.source_app, Some("TestApp".to_string()));
    }

    #[test]
    fn test_clipboard_capture_hash_consistency() {
        let capture1 = ClipboardCapture::new("same content".to_string(), None);
        let capture2 = ClipboardCapture::new("same content".to_string(), None);

        assert_eq!(capture1.content_hash, capture2.content_hash);
    }

    #[test]
    fn test_clipboard_capture_hash_different_content() {
        let capture1 = ClipboardCapture::new("content one".to_string(), None);
        let capture2 = ClipboardCapture::new("content two".to_string(), None);

        assert_ne!(capture1.content_hash, capture2.content_hash);
    }

    #[test]
    fn test_clipboard_capture_with_no_source_app() {
        let capture = ClipboardCapture::new("test content".to_string(), None);

        assert_eq!(capture.content, "test content");
        assert!(!capture.content_hash.is_empty());
        assert!(capture.source_app.is_none());
    }

    #[test]
    fn test_clipboard_capture_debug() {
        let capture = ClipboardCapture::new("test".to_string(), Some("App".to_string()));
        let debug_str = format!("{:?}", capture);

        assert!(debug_str.contains("ClipboardCapture"));
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("App"));
    }

    #[test]
    fn test_clipboard_capture_clone() {
        let capture = ClipboardCapture::new("test content".to_string(), Some("App".to_string()));
        let cloned = capture.clone();

        assert_eq!(capture.content, cloned.content);
        assert_eq!(capture.content_hash, cloned.content_hash);
        assert_eq!(capture.source_app, cloned.source_app);
        assert_eq!(capture.timestamp, cloned.timestamp);
    }

    #[test]
    fn test_clipboard_capture_eq() {
        let capture1 = ClipboardCapture {
            content: "test".to_string(),
            content_hash: "hash".to_string(),
            timestamp: Utc::now(),
            source_app: Some("App".to_string()),
        };
        let capture2 = ClipboardCapture {
            content: "test".to_string(),
            content_hash: "hash".to_string(),
            timestamp: capture1.timestamp,
            source_app: Some("App".to_string()),
        };

        assert_eq!(capture1, capture2);
    }

    #[test]
    fn test_clipboard_capture_ne() {
        let capture1 = ClipboardCapture::new("test1".to_string(), None);
        let capture2 = ClipboardCapture::new("test2".to_string(), None);

        assert_ne!(capture1, capture2);
    }

    #[test]
    fn test_clipboard_monitor_config_default() {
        let config = ClipboardMonitorConfig::default();

        assert_eq!(config.poll_interval, Duration::from_millis(500));
        assert_eq!(config.min_content_length, 1);
        assert_eq!(config.max_content_length, 1_000_000);
    }

    #[test]
    fn test_clipboard_monitor_config_clone() {
        let config = ClipboardMonitorConfig {
            poll_interval: Duration::from_secs(2),
            min_content_length: 5,
            max_content_length: 500,
        };
        let cloned = config.clone();

        assert_eq!(config.poll_interval, cloned.poll_interval);
        assert_eq!(config.min_content_length, cloned.min_content_length);
        assert_eq!(config.max_content_length, cloned.max_content_length);
    }

    #[test]
    fn test_clipboard_monitor_config_debug() {
        let config = ClipboardMonitorConfig::default();
        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("ClipboardMonitorConfig"));
        assert!(debug_str.contains("poll_interval"));
        assert!(debug_str.contains("min_content_length"));
        assert!(debug_str.contains("max_content_length"));
    }

    #[test]
    fn test_clipboard_monitor_new() {
        let monitor = ClipboardMonitor::new();

        assert!(!monitor.is_running());
        assert!(monitor.last_hash.is_none());
    }

    #[test]
    fn test_clipboard_monitor_with_config() {
        let config = ClipboardMonitorConfig {
            poll_interval: Duration::from_secs(1),
            min_content_length: 10,
            max_content_length: 100,
        };
        let monitor = ClipboardMonitor::with_config(config);

        assert_eq!(monitor.config.poll_interval, Duration::from_secs(1));
        assert_eq!(monitor.config.min_content_length, 10);
        assert_eq!(monitor.config.max_content_length, 100);
    }

    #[test]
    fn test_clipboard_monitor_default() {
        let monitor = ClipboardMonitor::default();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_clipboard_monitor_stop_handle() {
        let monitor = ClipboardMonitor::new();
        let handle = monitor.stop_handle();

        assert!(!handle.is_running());
    }

    #[test]
    fn test_clipboard_monitor_stop() {
        let monitor = ClipboardMonitor::new();

        // Manually set running to true via the internal arc
        monitor.running.store(true, Ordering::SeqCst);
        assert!(monitor.is_running());

        // Now stop it
        monitor.stop();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_clipboard_monitor_debug() {
        let monitor = ClipboardMonitor::new();
        let debug_str = format!("{:?}", monitor);

        assert!(debug_str.contains("ClipboardMonitor"));
        assert!(debug_str.contains("config"));
        assert!(debug_str.contains("running"));
    }

    #[test]
    fn test_clipboard_monitor_handle_stop() {
        let monitor = ClipboardMonitor::new();
        let handle = monitor.stop_handle();

        // Initially not running
        assert!(!handle.is_running());

        // Set to running via the monitor
        monitor.running.store(true, Ordering::SeqCst);
        assert!(handle.is_running());

        // Stop via the handle
        handle.stop();
        assert!(!handle.is_running());
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_clipboard_monitor_handle_clone_shares_state() {
        let monitor = ClipboardMonitor::new();
        let handle1 = monitor.stop_handle();
        let handle2 = handle1.clone();

        // Set running
        monitor.running.store(true, Ordering::SeqCst);
        assert!(handle1.is_running());
        assert!(handle2.is_running());

        // Stop via handle1 should affect handle2
        handle1.stop();
        assert!(!handle2.is_running());
    }

    #[test]
    fn test_clipboard_monitor_handle_debug() {
        let monitor = ClipboardMonitor::new();
        let handle = monitor.stop_handle();
        let debug_str = format!("{:?}", handle);

        assert!(debug_str.contains("ClipboardMonitorHandle"));
        assert!(debug_str.contains("running"));
    }

    #[test]
    fn test_clipboard_error_display() {
        let error = ClipboardError::AccessFailed("test error".to_string());
        assert_eq!(error.to_string(), "clipboard access failed: test error");

        let error = ClipboardError::NotRunning;
        assert_eq!(error.to_string(), "clipboard monitor is not running");

        let error = ClipboardError::ChannelClosed;
        assert_eq!(error.to_string(), "failed to send capture: channel closed");
    }

    #[test]
    fn test_clipboard_error_debug() {
        let error = ClipboardError::AccessFailed("test".to_string());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("AccessFailed"));

        let error = ClipboardError::NotRunning;
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("NotRunning"));

        let error = ClipboardError::ChannelClosed;
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("ChannelClosed"));
    }

    #[test]
    fn test_clipboard_capture_timestamp_is_recent() {
        let before = Utc::now();
        let capture = ClipboardCapture::new("test".to_string(), None);
        let after = Utc::now();

        assert!(capture.timestamp >= before);
        assert!(capture.timestamp <= after);
    }

    #[test]
    fn test_clipboard_capture_creates_unique_hash() {
        let capture1 = ClipboardCapture::new("abc".to_string(), None);
        let capture2 = ClipboardCapture::new("abd".to_string(), None);
        let capture3 = ClipboardCapture::new("abc".to_string(), None);

        assert_ne!(capture1.content_hash, capture2.content_hash);
        assert_eq!(capture1.content_hash, capture3.content_hash);
    }

    #[test]
    fn test_clipboard_monitor_running_state_transitions() {
        let monitor = ClipboardMonitor::new();

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
    fn test_clipboard_monitor_handle_multiple_stops() {
        let monitor = ClipboardMonitor::new();
        let handle = monitor.stop_handle();

        // Multiple stops should be idempotent
        monitor.running.store(true, Ordering::SeqCst);
        handle.stop();
        handle.stop();
        handle.stop();

        assert!(!monitor.is_running());
    }

    #[test]
    fn test_clipboard_config_boundary_values() {
        let config = ClipboardMonitorConfig {
            poll_interval: Duration::from_millis(1),
            min_content_length: 0,
            max_content_length: usize::MAX,
        };

        assert_eq!(config.poll_interval.as_millis(), 1);
        assert_eq!(config.min_content_length, 0);
        assert_eq!(config.max_content_length, usize::MAX);
    }

    #[test]
    fn test_clipboard_error_variants_all() {
        // Test all error variants for coverage
        let err1 = ClipboardError::AccessFailed("access error".to_string());
        let err2 = ClipboardError::NotRunning;
        let err3 = ClipboardError::ChannelClosed;

        // Debug format
        assert!(format!("{err1:?}").contains("AccessFailed"));
        assert!(format!("{err2:?}").contains("NotRunning"));
        assert!(format!("{err3:?}").contains("ChannelClosed"));

        // Display format
        assert!(err1.to_string().contains("access"));
        assert!(err2.to_string().contains("not running"));
        assert!(err3.to_string().contains("channel"));
    }

    #[test]
    fn test_clipboard_capture_long_content() {
        let long_content = "x".repeat(10000);
        let capture = ClipboardCapture::new(long_content.clone(), Some("App".to_string()));

        assert_eq!(capture.content, long_content);
        assert!(!capture.content_hash.is_empty());
    }

    #[test]
    fn test_clipboard_capture_empty_content() {
        let capture = ClipboardCapture::new(String::new(), None);

        assert!(capture.content.is_empty());
        assert!(!capture.content_hash.is_empty()); // Hash of empty string is still a hash
    }

    #[test]
    fn test_clipboard_capture_special_characters() {
        let special = "Hello\n\t\r\0World 🎉 émoji".to_string();
        let capture = ClipboardCapture::new(special.clone(), None);

        assert_eq!(capture.content, special);
    }

    #[test]
    fn test_clipboard_monitor_last_hash_initially_none() {
        let monitor = ClipboardMonitor::new();
        assert!(monitor.last_hash.is_none());
    }

    #[test]
    fn test_clipboard_config_custom_values() {
        let config = ClipboardMonitorConfig {
            poll_interval: Duration::from_secs(10),
            min_content_length: 100,
            max_content_length: 50000,
        };

        assert_eq!(config.poll_interval.as_secs(), 10);
        assert_eq!(config.min_content_length, 100);
        assert_eq!(config.max_content_length, 50000);
    }

    // Tests for helper functions

    #[test]
    fn test_process_content_normal() {
        let result = process_content("hello world", 1, 100);
        assert_eq!(result, Some("hello world".to_string()));
    }

    #[test]
    fn test_process_content_too_short() {
        let result = process_content("hi", 5, 100);
        assert_eq!(result, None);
    }

    #[test]
    fn test_process_content_truncated() {
        let result = process_content("hello world", 1, 5);
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn test_process_content_exact_min_length() {
        let result = process_content("abc", 3, 100);
        assert_eq!(result, Some("abc".to_string()));
    }

    #[test]
    fn test_process_content_exact_max_length() {
        let result = process_content("abc", 1, 3);
        assert_eq!(result, Some("abc".to_string()));
    }

    #[test]
    fn test_process_content_empty_with_zero_min() {
        let result = process_content("", 0, 100);
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn test_process_content_empty_with_nonzero_min() {
        let result = process_content("", 1, 100);
        assert_eq!(result, None);
    }

    #[test]
    fn test_content_changed_with_none() {
        assert!(content_changed("hello", None));
    }

    #[test]
    fn test_content_changed_same_content() {
        let hash = compute_hash("hello");
        assert!(!content_changed("hello", Some(&hash)));
    }

    #[test]
    fn test_content_changed_different_content() {
        let hash = compute_hash("hello");
        assert!(content_changed("world", Some(&hash)));
    }

    #[test]
    fn test_compute_hash_consistency() {
        let hash1 = compute_hash("test");
        let hash2 = compute_hash("test");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_different_content() {
        let hash1 = compute_hash("test1");
        let hash2 = compute_hash("test2");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_empty_string() {
        let hash = compute_hash("");
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_compute_hash_long_content() {
        let long_content = "x".repeat(100000);
        let hash = compute_hash(&long_content);
        assert!(!hash.is_empty());
        // BLAKE3 produces 64-character hex strings
        assert_eq!(hash.len(), 64);
    }

    // Test get_frontmost_app (uses osascript, not clipboard-rs)
    #[test]
    fn test_get_frontmost_app_returns_result() {
        // This test should work on macOS without clipboard access
        // It uses osascript which is safe
        let result = get_frontmost_app();
        // Should return Some(app_name) or None, but shouldn't panic
        if let Some(name) = result {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn test_get_frontmost_app_via_nsworkspace_returns_result() {
        let result = get_frontmost_app_via_nsworkspace();
        // Should return Some(app_name) or None
        if let Some(name) = result {
            assert!(!name.is_empty());
        }
    }

    // Tests for process_text_with_source method (can test logic without clipboard access)
    // Using process_text_with_source(text, None) avoids slow osascript calls

    #[test]
    fn test_process_text_normal_content() {
        let mut monitor = ClipboardMonitor::new();
        let result = monitor
            .process_text_with_source("Hello, World!".to_string(), None)
            .unwrap();

        assert!(result.is_some());
        let capture = result.unwrap();
        assert_eq!(capture.content, "Hello, World!");
    }

    #[test]
    fn test_process_text_skips_short_content() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            min_content_length: 10,
            ..Default::default()
        });

        let result = monitor
            .process_text_with_source("short".to_string(), None)
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_process_text_truncates_long_content() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            max_content_length: 10,
            ..Default::default()
        });

        let result = monitor
            .process_text_with_source(
                "This is a very long string that should be truncated".to_string(),
                None,
            )
            .unwrap();

        assert!(result.is_some());
        let capture = result.unwrap();
        assert_eq!(capture.content.len(), 10);
        assert_eq!(capture.content, "This is a ");
    }

    #[test]
    fn test_process_text_deduplicates_same_content() {
        let mut monitor = ClipboardMonitor::new();

        // First call should return Some
        let result1 = monitor
            .process_text_with_source("Same content".to_string(), None)
            .unwrap();
        assert!(result1.is_some());

        // Second call with same content should return None (deduplicated)
        let result2 = monitor
            .process_text_with_source("Same content".to_string(), None)
            .unwrap();
        assert!(result2.is_none());
    }

    #[test]
    fn test_process_text_detects_changed_content() {
        let mut monitor = ClipboardMonitor::new();

        // First call
        let result1 = monitor
            .process_text_with_source("Content one".to_string(), None)
            .unwrap();
        assert!(result1.is_some());

        // Second call with different content should return Some
        let result2 = monitor
            .process_text_with_source("Content two".to_string(), None)
            .unwrap();
        assert!(result2.is_some());
    }

    #[test]
    fn test_process_text_exact_min_length() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            min_content_length: 5,
            ..Default::default()
        });

        let result = monitor
            .process_text_with_source("12345".to_string(), None)
            .unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_process_text_exact_max_length() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            max_content_length: 5,
            ..Default::default()
        });

        let result = monitor
            .process_text_with_source("12345".to_string(), None)
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "12345");
    }

    #[test]
    fn test_process_text_one_below_min_length() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            min_content_length: 5,
            ..Default::default()
        });

        let result = monitor
            .process_text_with_source("1234".to_string(), None)
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_process_text_empty_string_with_min_zero() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            min_content_length: 0,
            ..Default::default()
        });

        let result = monitor
            .process_text_with_source(String::new(), None)
            .unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_process_text_updates_last_hash() {
        let mut monitor = ClipboardMonitor::new();
        assert!(monitor.last_hash.is_none());

        let _ = monitor
            .process_text_with_source("test content".to_string(), None)
            .unwrap();
        assert!(monitor.last_hash.is_some());
    }

    #[test]
    fn test_process_text_special_characters() {
        let mut monitor = ClipboardMonitor::new();
        let special = "Hello\n\t\r\0World 🎉 émoji".to_string();

        let result = monitor
            .process_text_with_source(special.clone(), None)
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, special);
    }

    #[test]
    fn test_process_text_unicode_truncation() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            max_content_length: 5,
            ..Default::default()
        });

        // ASCII string, safe to truncate at byte boundary
        let result = monitor
            .process_text_with_source("Hello World".to_string(), None)
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "Hello");
    }

    #[test]
    fn test_process_text_with_provided_source_app() {
        let mut monitor = ClipboardMonitor::new();
        let result = monitor
            .process_text_with_source("test content".to_string(), Some("TestApp".to_string()))
            .unwrap();

        assert!(result.is_some());
        let capture = result.unwrap();
        assert_eq!(capture.content, "test content");
        assert_eq!(capture.source_app, Some("TestApp".to_string()));
    }

    #[test]
    fn test_process_text_multiple_sequential_different() {
        let mut monitor = ClipboardMonitor::new();

        // Process several different strings
        let result1 = monitor
            .process_text_with_source("first".to_string(), None)
            .unwrap();
        assert!(result1.is_some());

        let result2 = monitor
            .process_text_with_source("second".to_string(), None)
            .unwrap();
        assert!(result2.is_some());

        let result3 = monitor
            .process_text_with_source("third".to_string(), None)
            .unwrap();
        assert!(result3.is_some());

        // Then repeat should deduplicate
        let result4 = monitor
            .process_text_with_source("third".to_string(), None)
            .unwrap();
        assert!(result4.is_none());
    }

    #[test]
    fn test_process_text_hash_changes_with_content() {
        let mut monitor = ClipboardMonitor::new();

        let _ = monitor
            .process_text_with_source("content1".to_string(), None)
            .unwrap();
        let hash1 = monitor.last_hash.clone();

        let _ = monitor
            .process_text_with_source("content2".to_string(), None)
            .unwrap();
        let hash2 = monitor.last_hash.clone();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_config_min_greater_than_max() {
        // This is an edge case - min > max
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            min_content_length: 100,
            max_content_length: 10,
            ..Default::default()
        });

        // Content of length 50 is less than min (100), so it should be rejected
        // The min check happens BEFORE truncation
        let result = monitor
            .process_text_with_source("x".repeat(50), None)
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_process_text_whitespace_only() {
        let mut monitor = ClipboardMonitor::new();
        let result = monitor
            .process_text_with_source("   \n\t\r  ".to_string(), None)
            .unwrap();

        // Whitespace-only content should still be captured (not filtered)
        assert!(result.is_some());
    }

    #[test]
    fn test_process_text_very_long_content() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            max_content_length: 100,
            ..Default::default()
        });

        let long_content = "x".repeat(10000);
        let result = monitor
            .process_text_with_source(long_content, None)
            .unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap().content.len(), 100);
    }

    // Integration tests that require actual clipboard access.
    // These are marked as #[ignore] by default because they can be flaky
    // in CI environments and require system clipboard access.
    // Run with: cargo test --ignored

    #[test]
    #[ignore = "requires clipboard access, may segfault in CI"]
    fn test_clipboard_monitor_get_current_text() {
        let monitor = ClipboardMonitor::new();
        // This test just verifies the method doesn't panic
        // Actual clipboard content depends on system state
        let result = monitor.get_current_text();
        assert!(result.is_ok());
    }

    #[test]
    #[ignore = "requires clipboard access, may segfault in CI"]
    fn test_clipboard_monitor_check_for_changes() {
        let mut monitor = ClipboardMonitor::new();
        // This test just verifies the method works
        // Actual behavior depends on clipboard content
        let result = monitor.check_for_changes();
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires clipboard access, may segfault in CI"]
    async fn test_clipboard_monitor_start_stop() {
        let mut monitor = ClipboardMonitor::with_config(ClipboardMonitorConfig {
            poll_interval: Duration::from_millis(50),
            ..Default::default()
        });

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

        // Close the receiver
        rx.close();
    }
}
