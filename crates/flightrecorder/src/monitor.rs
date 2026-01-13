//! Platform-agnostic capture monitoring abstraction.
//!
//! This module defines the core traits and types for capture monitoring
//! that platform-specific implementations must fulfill.

use std::time::Duration;

use thiserror::Error;
use tokio::sync::mpsc;

use crate::Capture;

/// Errors that can occur during capture monitoring.
#[derive(Debug, Error)]
pub enum MonitorError {
    /// The monitor failed to start.
    #[error("failed to start monitor: {0}")]
    StartFailed(String),

    /// The monitor failed to stop.
    #[error("failed to stop monitor: {0}")]
    StopFailed(String),

    /// Permission required to run the monitor.
    #[error("permission required: {0}")]
    PermissionRequired(String),

    /// The monitor is already running.
    #[error("monitor already running")]
    AlreadyRunning,

    /// The monitor is not running.
    #[error("monitor not running")]
    NotRunning,

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Result type for monitor operations.
pub type Result<T> = std::result::Result<T, MonitorError>;

/// The type of capture source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MonitorType {
    /// Monitors clipboard changes.
    Clipboard,

    /// Monitors focused text fields via accessibility APIs.
    Accessibility,

    /// Monitors keystrokes (fallback mode).
    Keystroke,
}

impl std::fmt::Display for MonitorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clipboard => write!(f, "clipboard"),
            Self::Accessibility => write!(f, "accessibility"),
            Self::Keystroke => write!(f, "keystroke"),
        }
    }
}

/// Configuration for a capture monitor.
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// The type of monitor.
    pub monitor_type: MonitorType,

    /// Interval between capture checks.
    pub poll_interval: Duration,

    /// Whether this monitor is enabled.
    pub enabled: bool,

    /// Minimum content length to capture.
    pub min_content_length: usize,

    /// Maximum content length to capture.
    pub max_content_length: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            monitor_type: MonitorType::Clipboard,
            poll_interval: Duration::from_millis(500),
            enabled: true,
            min_content_length: 1,
            max_content_length: 1_000_000,
        }
    }
}

/// Status of a capture monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorStatus {
    /// The type of monitor.
    pub monitor_type: MonitorType,

    /// Whether the monitor is currently running.
    pub is_running: bool,

    /// Whether the required permissions are granted.
    pub has_permission: bool,

    /// Number of captures since startup.
    pub capture_count: u64,

    /// Human-readable status message.
    pub message: String,
}

impl MonitorStatus {
    /// Create a new status for a stopped monitor.
    #[must_use]
    pub fn stopped(monitor_type: MonitorType) -> Self {
        Self {
            monitor_type,
            is_running: false,
            has_permission: true,
            capture_count: 0,
            message: "Monitor stopped".to_string(),
        }
    }

    /// Create a new status for a running monitor.
    #[must_use]
    pub fn running(monitor_type: MonitorType, capture_count: u64) -> Self {
        Self {
            monitor_type,
            is_running: true,
            has_permission: true,
            capture_count,
            message: "Monitor running".to_string(),
        }
    }

    /// Create a status indicating missing permissions.
    #[must_use]
    pub fn permission_required(monitor_type: MonitorType, message: &str) -> Self {
        Self {
            monitor_type,
            is_running: false,
            has_permission: false,
            capture_count: 0,
            message: message.to_string(),
        }
    }
}

/// A trait for platform-specific capture monitors.
///
/// Implementors provide the actual capture logic for a specific platform
/// and capture source (clipboard, accessibility, etc.).
#[async_trait::async_trait]
pub trait CaptureMonitor: Send + Sync {
    /// Get the type of this monitor.
    fn monitor_type(&self) -> MonitorType;

    /// Check if the monitor is currently running.
    fn is_running(&self) -> bool;

    /// Check if required permissions are available.
    fn has_permission(&self) -> bool;

    /// Get the current status of the monitor.
    fn status(&self) -> MonitorStatus;

    /// Start the monitor and begin sending captures.
    ///
    /// # Arguments
    ///
    /// * `tx` - Channel to send captures through
    ///
    /// # Errors
    ///
    /// Returns an error if the monitor fails to start.
    async fn start(&mut self, tx: mpsc::Sender<Capture>) -> Result<()>;

    /// Stop the monitor.
    ///
    /// # Errors
    ///
    /// Returns an error if the monitor fails to stop cleanly.
    fn stop(&self) -> Result<()>;
}

/// A handle to control capture monitors.
///
/// This is a lightweight, cloneable handle that can be used to control
/// monitors from multiple tasks.
#[derive(Debug, Clone)]
pub struct MonitorHandle {
    monitor_type: MonitorType,
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl MonitorHandle {
    /// Create a new monitor handle.
    #[must_use]
    pub fn new(monitor_type: MonitorType) -> Self {
        Self {
            monitor_type,
            stop_signal: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Get the monitor type.
    #[must_use]
    pub fn monitor_type(&self) -> MonitorType {
        self.monitor_type
    }

    /// Signal the monitor to stop.
    pub fn stop(&self) {
        self.stop_signal
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check if the stop signal has been sent.
    #[must_use]
    pub fn should_stop(&self) -> bool {
        self.stop_signal.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Reset the stop signal.
    pub fn reset(&self) {
        self.stop_signal
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A collection of capture monitors that can be managed together.
#[derive(Debug, Default)]
pub struct MonitorManager {
    handles: Vec<MonitorHandle>,
}

impl MonitorManager {
    /// Create a new monitor manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a monitor handle to manage.
    pub fn add(&mut self, handle: MonitorHandle) {
        self.handles.push(handle);
    }

    /// Stop all monitors.
    pub fn stop_all(&self) {
        for handle in &self.handles {
            handle.stop();
        }
    }

    /// Get the number of managed monitors.
    #[must_use]
    pub fn count(&self) -> usize {
        self.handles.len()
    }

    /// Check if any monitors are still running (haven't been signaled to stop).
    #[must_use]
    pub fn any_running(&self) -> bool {
        self.handles.iter().any(|h| !h.should_stop())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_type_display() {
        assert_eq!(MonitorType::Clipboard.to_string(), "clipboard");
        assert_eq!(MonitorType::Accessibility.to_string(), "accessibility");
        assert_eq!(MonitorType::Keystroke.to_string(), "keystroke");
    }

    #[test]
    fn test_monitor_config_default() {
        let config = MonitorConfig::default();
        assert_eq!(config.monitor_type, MonitorType::Clipboard);
        assert_eq!(config.poll_interval, Duration::from_millis(500));
        assert!(config.enabled);
    }

    #[test]
    fn test_monitor_status_stopped() {
        let status = MonitorStatus::stopped(MonitorType::Clipboard);
        assert_eq!(status.monitor_type, MonitorType::Clipboard);
        assert!(!status.is_running);
        assert!(status.has_permission);
        assert_eq!(status.capture_count, 0);
    }

    #[test]
    fn test_monitor_status_running() {
        let status = MonitorStatus::running(MonitorType::Accessibility, 42);
        assert_eq!(status.monitor_type, MonitorType::Accessibility);
        assert!(status.is_running);
        assert!(status.has_permission);
        assert_eq!(status.capture_count, 42);
    }

    #[test]
    fn test_monitor_status_permission_required() {
        let status = MonitorStatus::permission_required(MonitorType::Accessibility, "Need access");
        assert!(!status.is_running);
        assert!(!status.has_permission);
        assert!(status.message.contains("Need access"));
    }

    #[test]
    fn test_monitor_handle_new() {
        let handle = MonitorHandle::new(MonitorType::Clipboard);
        assert_eq!(handle.monitor_type(), MonitorType::Clipboard);
        assert!(!handle.should_stop());
    }

    #[test]
    fn test_monitor_handle_stop() {
        let handle = MonitorHandle::new(MonitorType::Clipboard);
        assert!(!handle.should_stop());

        handle.stop();
        assert!(handle.should_stop());
    }

    #[test]
    fn test_monitor_handle_reset() {
        let handle = MonitorHandle::new(MonitorType::Clipboard);
        handle.stop();
        assert!(handle.should_stop());

        handle.reset();
        assert!(!handle.should_stop());
    }

    #[test]
    fn test_monitor_handle_clone() {
        let handle1 = MonitorHandle::new(MonitorType::Clipboard);
        let handle2 = handle1.clone();

        handle1.stop();
        assert!(handle2.should_stop()); // Shares the same signal
    }

    #[test]
    fn test_monitor_manager_new() {
        let manager = MonitorManager::new();
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_monitor_manager_add() {
        let mut manager = MonitorManager::new();
        manager.add(MonitorHandle::new(MonitorType::Clipboard));
        manager.add(MonitorHandle::new(MonitorType::Accessibility));
        assert_eq!(manager.count(), 2);
    }

    #[test]
    fn test_monitor_manager_stop_all() {
        let mut manager = MonitorManager::new();
        let handle1 = MonitorHandle::new(MonitorType::Clipboard);
        let handle2 = MonitorHandle::new(MonitorType::Accessibility);

        manager.add(handle1.clone());
        manager.add(handle2.clone());

        assert!(!handle1.should_stop());
        assert!(!handle2.should_stop());

        manager.stop_all();

        assert!(handle1.should_stop());
        assert!(handle2.should_stop());
    }

    #[test]
    fn test_monitor_manager_any_running() {
        let mut manager = MonitorManager::new();
        let handle1 = MonitorHandle::new(MonitorType::Clipboard);
        let handle2 = MonitorHandle::new(MonitorType::Accessibility);

        manager.add(handle1.clone());
        manager.add(handle2.clone());

        assert!(manager.any_running());

        handle1.stop();
        assert!(manager.any_running()); // handle2 still running

        handle2.stop();
        assert!(!manager.any_running());
    }

    #[test]
    fn test_monitor_error_display() {
        assert!(MonitorError::StartFailed("test".to_string())
            .to_string()
            .contains("start"));
        assert!(MonitorError::StopFailed("test".to_string())
            .to_string()
            .contains("stop"));
        assert!(MonitorError::PermissionRequired("test".to_string())
            .to_string()
            .contains("permission"));
        assert!(MonitorError::AlreadyRunning
            .to_string()
            .contains("already running"));
        assert!(MonitorError::NotRunning.to_string().contains("not running"));
        assert!(MonitorError::Internal("test".to_string())
            .to_string()
            .contains("internal"));
    }
}
