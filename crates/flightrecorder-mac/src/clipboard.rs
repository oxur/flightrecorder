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

        // Try to get source application
        let source_app = get_frontmost_app();

        Ok(Some(ClipboardCapture::new(content, source_app)))
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
fn get_frontmost_app() -> Option<String> {
    // Use NSWorkspace to get the frontmost application
    // This is a safe approach that doesn't require accessibility permissions
    get_frontmost_app_via_nsworkspace()
}

/// Get frontmost app using `NSWorkspace` (safe, no special permissions needed).
#[must_use]
fn get_frontmost_app_via_nsworkspace() -> Option<String> {
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
