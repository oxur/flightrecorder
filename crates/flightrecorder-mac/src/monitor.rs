//! macOS-specific monitor implementations.
//!
//! This module provides platform-specific implementations of the
//! `CaptureMonitor` trait for macOS.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::mpsc;
use tracing::{debug, trace};

use crate::accessibility::{AccessibilityMonitor, AccessibilityMonitorConfig};
use crate::clipboard::{ClipboardMonitor, ClipboardMonitorConfig};
use crate::permissions;

// Re-export types from the main crate that we need
// The actual types would come from flightrecorder crate
// For now we define compatible types here

/// Capture type enum matching the main crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureType {
    /// Clipboard capture.
    Clipboard,
    /// Text field capture via accessibility.
    TextField,
    /// Keystroke capture (fallback).
    Keystroke,
}

/// A captured piece of content.
#[derive(Debug, Clone)]
pub struct CaptureData {
    /// The captured content.
    pub content: String,

    /// Hash of the content.
    pub content_hash: String,

    /// Capture timestamp.
    pub timestamp: chrono::DateTime<Utc>,

    /// Source application.
    pub source_app: Option<String>,

    /// Type of capture.
    pub capture_type: CaptureType,
}

/// Monitor type enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorType {
    /// Clipboard monitor.
    Clipboard,
    /// Accessibility monitor.
    Accessibility,
}

/// Status of a monitor.
#[derive(Debug, Clone)]
pub struct MonitorStatus {
    /// Monitor type.
    pub monitor_type: MonitorType,

    /// Whether running.
    pub is_running: bool,

    /// Whether has permission.
    pub has_permission: bool,

    /// Capture count.
    pub capture_count: u64,

    /// Status message.
    pub message: String,
}

/// macOS clipboard monitor adapter.
///
/// Wraps the internal `ClipboardMonitor` and provides a standardized interface.
#[derive(Debug)]
pub struct MacClipboardMonitor {
    inner: ClipboardMonitor,
    running: Arc<AtomicBool>,
    capture_count: Arc<AtomicU64>,
}

impl MacClipboardMonitor {
    /// Create a new macOS clipboard monitor.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ClipboardMonitorConfig::default())
    }

    /// Create with custom configuration.
    #[must_use]
    pub fn with_config(config: ClipboardMonitorConfig) -> Self {
        Self {
            inner: ClipboardMonitor::with_config(config),
            running: Arc::new(AtomicBool::new(false)),
            capture_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get the monitor type.
    #[must_use]
    pub const fn monitor_type(&self) -> MonitorType {
        MonitorType::Clipboard
    }

    /// Check if running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Check permission (clipboard doesn't need special permission).
    #[must_use]
    pub const fn has_permission(&self) -> bool {
        true // Clipboard access doesn't require special permissions on macOS
    }

    /// Get status.
    #[must_use]
    pub fn status(&self) -> MonitorStatus {
        MonitorStatus {
            monitor_type: MonitorType::Clipboard,
            is_running: self.is_running(),
            has_permission: true,
            capture_count: self.capture_count.load(Ordering::SeqCst),
            message: if self.is_running() {
                "Monitoring clipboard".to_string()
            } else {
                "Not running".to_string()
            },
        }
    }

    /// Start the monitor.
    ///
    /// # Errors
    ///
    /// Returns an error if the monitor fails to start.
    pub async fn start(&mut self, tx: mpsc::Sender<CaptureData>) -> Result<(), String> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err("Already running".to_string());
        }

        debug!("Starting macOS clipboard monitor");

        let (internal_tx, mut internal_rx) =
            mpsc::channel::<crate::clipboard::ClipboardCapture>(100);
        let capture_count = Arc::clone(&self.capture_count);
        let _running = Arc::clone(&self.running);

        // Spawn a task to convert internal captures to CaptureData
        tokio::spawn(async move {
            while let Some(capture) = internal_rx.recv().await {
                let data = CaptureData {
                    content: capture.content,
                    content_hash: capture.content_hash,
                    timestamp: capture.timestamp,
                    source_app: capture.source_app,
                    capture_type: CaptureType::Clipboard,
                };

                capture_count.fetch_add(1, Ordering::SeqCst);

                if tx.send(data).await.is_err() {
                    debug!("Output channel closed");
                    break;
                }
            }
        });

        // Start the internal monitor
        if let Err(e) = self.inner.start(internal_tx).await {
            self.running.store(false, Ordering::SeqCst);
            return Err(e.to_string());
        }

        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Stop the monitor.
    pub fn stop(&self) {
        debug!("Stopping macOS clipboard monitor");
        self.inner.stop();
        self.running.store(false, Ordering::SeqCst);
    }

    /// Get a stop handle.
    #[must_use]
    pub fn stop_handle(&self) -> MacMonitorHandle {
        MacMonitorHandle {
            running: Arc::clone(&self.running),
            inner_handle: Some(self.inner.stop_handle()),
        }
    }
}

impl Default for MacClipboardMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// macOS accessibility monitor adapter.
#[derive(Debug)]
pub struct MacAccessibilityMonitor {
    inner: AccessibilityMonitor,
    running: Arc<AtomicBool>,
    capture_count: Arc<AtomicU64>,
}

impl MacAccessibilityMonitor {
    /// Create a new macOS accessibility monitor.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AccessibilityMonitorConfig::default())
    }

    /// Create with custom configuration.
    #[must_use]
    pub fn with_config(config: AccessibilityMonitorConfig) -> Self {
        Self {
            inner: AccessibilityMonitor::with_config(config),
            running: Arc::new(AtomicBool::new(false)),
            capture_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get the monitor type.
    #[must_use]
    pub const fn monitor_type(&self) -> MonitorType {
        MonitorType::Accessibility
    }

    /// Check if running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Check permission.
    #[must_use]
    pub fn has_permission(&self) -> bool {
        permissions::is_accessibility_enabled()
    }

    /// Get status.
    #[must_use]
    pub fn status(&self) -> MonitorStatus {
        let has_perm = self.has_permission();
        MonitorStatus {
            monitor_type: MonitorType::Accessibility,
            is_running: self.is_running(),
            has_permission: has_perm,
            capture_count: self.capture_count.load(Ordering::SeqCst),
            message: if !has_perm {
                "Accessibility permission required".to_string()
            } else if self.is_running() {
                "Monitoring text fields".to_string()
            } else {
                "Not running".to_string()
            },
        }
    }

    /// Start the monitor.
    ///
    /// # Errors
    ///
    /// Returns an error if permission is not granted or start fails.
    pub async fn start(&mut self, tx: mpsc::Sender<CaptureData>) -> Result<(), String> {
        if !self.has_permission() {
            return Err("Accessibility permission required".to_string());
        }

        if self.running.swap(true, Ordering::SeqCst) {
            return Err("Already running".to_string());
        }

        debug!("Starting macOS accessibility monitor");

        let (internal_tx, mut internal_rx) =
            mpsc::channel::<crate::accessibility::TextFieldCapture>(100);
        let capture_count = Arc::clone(&self.capture_count);

        // Spawn a task to convert internal captures to CaptureData
        tokio::spawn(async move {
            while let Some(capture) = internal_rx.recv().await {
                let data = CaptureData {
                    content: capture.content,
                    content_hash: capture.content_hash,
                    timestamp: capture.timestamp,
                    source_app: capture.source_app,
                    capture_type: CaptureType::TextField,
                };

                capture_count.fetch_add(1, Ordering::SeqCst);
                trace!(
                    count = capture_count.load(Ordering::SeqCst),
                    "Capture recorded"
                );

                if tx.send(data).await.is_err() {
                    debug!("Output channel closed");
                    break;
                }
            }
        });

        // Start the internal monitor
        if let Err(e) = self.inner.start(internal_tx).await {
            self.running.store(false, Ordering::SeqCst);
            return Err(e.to_string());
        }

        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Stop the monitor.
    pub fn stop(&self) {
        debug!("Stopping macOS accessibility monitor");
        self.inner.stop();
        self.running.store(false, Ordering::SeqCst);
    }

    /// Get a stop handle.
    #[must_use]
    pub fn stop_handle(&self) -> MacMonitorHandle {
        MacMonitorHandle {
            running: Arc::clone(&self.running),
            inner_handle: None, // Accessibility monitor uses different handle type
        }
    }
}

impl Default for MacAccessibilityMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle to stop macOS monitors.
#[derive(Debug, Clone)]
pub struct MacMonitorHandle {
    running: Arc<AtomicBool>,
    inner_handle: Option<crate::clipboard::ClipboardMonitorHandle>,
}

impl MacMonitorHandle {
    /// Stop the monitor.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(ref handle) = self.inner_handle {
            handle.stop();
        }
    }

    /// Check if still running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Create all available monitors for macOS.
#[must_use]
pub fn create_monitors() -> (MacClipboardMonitor, MacAccessibilityMonitor) {
    (MacClipboardMonitor::new(), MacAccessibilityMonitor::new())
}

/// Check if accessibility permissions are available.
#[must_use]
pub fn check_accessibility_permission() -> bool {
    permissions::is_accessibility_enabled()
}

/// Request accessibility permissions (shows system prompt).
#[must_use]
pub fn request_accessibility_permission() -> bool {
    permissions::request_accessibility_permission()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_type_debug() {
        assert_eq!(format!("{:?}", CaptureType::Clipboard), "Clipboard");
        assert_eq!(format!("{:?}", CaptureType::TextField), "TextField");
    }

    #[test]
    fn test_monitor_type_debug() {
        assert_eq!(format!("{:?}", MonitorType::Clipboard), "Clipboard");
        assert_eq!(format!("{:?}", MonitorType::Accessibility), "Accessibility");
    }

    #[test]
    fn test_mac_clipboard_monitor_new() {
        let monitor = MacClipboardMonitor::new();
        assert!(!monitor.is_running());
        assert!(monitor.has_permission());
        assert_eq!(monitor.monitor_type(), MonitorType::Clipboard);
    }

    #[test]
    fn test_mac_clipboard_monitor_default() {
        let monitor = MacClipboardMonitor::default();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_mac_clipboard_monitor_status() {
        let monitor = MacClipboardMonitor::new();
        let status = monitor.status();

        assert_eq!(status.monitor_type, MonitorType::Clipboard);
        assert!(!status.is_running);
        assert!(status.has_permission);
        assert_eq!(status.capture_count, 0);
    }

    #[test]
    fn test_mac_accessibility_monitor_new() {
        let monitor = MacAccessibilityMonitor::new();
        assert!(!monitor.is_running());
        assert_eq!(monitor.monitor_type(), MonitorType::Accessibility);
    }

    #[test]
    fn test_mac_accessibility_monitor_default() {
        let monitor = MacAccessibilityMonitor::default();
        assert!(!monitor.is_running());
    }

    #[test]
    fn test_mac_accessibility_monitor_status() {
        let monitor = MacAccessibilityMonitor::new();
        let status = monitor.status();

        assert_eq!(status.monitor_type, MonitorType::Accessibility);
        assert!(!status.is_running);
        // Permission status depends on system state
    }

    #[test]
    fn test_mac_monitor_handle() {
        let monitor = MacClipboardMonitor::new();
        let handle = monitor.stop_handle();

        assert!(!handle.is_running());
    }

    #[test]
    fn test_create_monitors() {
        let (clipboard, accessibility) = create_monitors();
        assert!(!clipboard.is_running());
        assert!(!accessibility.is_running());
    }
}
