//! Core capture types for flightrecorder.
//!
//! This module defines the fundamental data structures for representing
//! captured text input from various sources.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The type of capture that produced this record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureType {
    /// Text copied to the system clipboard.
    Clipboard,
    /// Text captured from a focused text field via accessibility APIs.
    TextField,
    /// Text reconstructed from keystroke events (fallback mode).
    Keystroke,
}

impl std::fmt::Display for CaptureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clipboard => write!(f, "clipboard"),
            Self::TextField => write!(f, "text_field"),
            Self::Keystroke => write!(f, "keystroke"),
        }
    }
}

/// A captured piece of text input.
///
/// Represents a single capture event with metadata about when, where,
/// and how the text was captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    /// Unique identifier for this capture (assigned by storage layer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,

    /// When this capture occurred.
    pub timestamp: DateTime<Utc>,

    /// The application that was the source of this capture (if detectable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,

    /// The captured text content.
    pub content: String,

    /// BLAKE3 hash of the content for deduplication.
    pub content_hash: String,

    /// How this text was captured.
    pub capture_type: CaptureType,
}

impl Capture {
    /// Create a new capture with the given content.
    ///
    /// Automatically computes the content hash and sets the timestamp to now.
    #[must_use]
    pub fn new(content: String, capture_type: CaptureType, source_app: Option<String>) -> Self {
        let content_hash = Self::compute_hash(&content);
        Self {
            id: None,
            timestamp: Utc::now(),
            source_app,
            content,
            content_hash,
            capture_type,
        }
    }

    /// Compute the BLAKE3 hash of the given content.
    #[must_use]
    pub fn compute_hash(content: &str) -> String {
        blake3::hash(content.as_bytes()).to_hex().to_string()
    }

    /// Check if this capture's content matches the given hash.
    #[must_use]
    pub fn matches_hash(&self, hash: &str) -> bool {
        self.content_hash == hash
    }

    /// Get the length of the captured content.
    #[must_use]
    pub fn content_len(&self) -> usize {
        self.content.len()
    }

    /// Check if the capture content is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

/// Trait for platform-specific capture sources.
///
/// Implementors of this trait provide the actual mechanism for capturing
/// text input on a specific platform (e.g., clipboard monitoring, accessibility APIs).
pub trait CaptureSource: Send + Sync {
    /// The name of this capture source (for logging/debugging).
    fn name(&self) -> &'static str;

    /// The type of captures this source produces.
    fn capture_type(&self) -> CaptureType;

    /// Start the capture source.
    ///
    /// This should begin monitoring for text input and sending captures
    /// through the provided channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the capture source fails to start, such as when
    /// required permissions are missing or platform APIs are unavailable.
    fn start(
        &mut self,
        sender: tokio::sync::mpsc::Sender<Capture>,
    ) -> Result<(), crate::error::Error>;

    /// Stop the capture source.
    ///
    /// # Errors
    ///
    /// Returns an error if the capture source fails to stop cleanly.
    fn stop(&mut self) -> Result<(), crate::error::Error>;

    /// Check if the capture source is currently running.
    fn is_running(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_type_display() {
        assert_eq!(CaptureType::Clipboard.to_string(), "clipboard");
        assert_eq!(CaptureType::TextField.to_string(), "text_field");
        assert_eq!(CaptureType::Keystroke.to_string(), "keystroke");
    }

    #[test]
    fn test_capture_new() {
        let capture = Capture::new(
            "Hello, world!".to_string(),
            CaptureType::Clipboard,
            Some("TestApp".to_string()),
        );

        assert!(capture.id.is_none());
        assert_eq!(capture.content, "Hello, world!");
        assert_eq!(capture.capture_type, CaptureType::Clipboard);
        assert_eq!(capture.source_app, Some("TestApp".to_string()));
        assert!(!capture.content_hash.is_empty());
    }

    #[test]
    fn test_capture_hash_consistency() {
        let content = "Test content";
        let hash1 = Capture::compute_hash(content);
        let hash2 = Capture::compute_hash(content);
        assert_eq!(hash1, hash2);

        let different_hash = Capture::compute_hash("Different content");
        assert_ne!(hash1, different_hash);
    }

    #[test]
    fn test_capture_matches_hash() {
        let capture = Capture::new("Test".to_string(), CaptureType::TextField, None);
        let hash = Capture::compute_hash("Test");
        assert!(capture.matches_hash(&hash));
        assert!(!capture.matches_hash("invalid_hash"));
    }

    #[test]
    fn test_capture_content_len() {
        let capture = Capture::new("Hello".to_string(), CaptureType::Clipboard, None);
        assert_eq!(capture.content_len(), 5);
    }

    #[test]
    fn test_capture_is_empty() {
        let empty = Capture::new(String::new(), CaptureType::Clipboard, None);
        assert!(empty.is_empty());

        let not_empty = Capture::new("x".to_string(), CaptureType::Clipboard, None);
        assert!(!not_empty.is_empty());
    }

    #[test]
    fn test_capture_serialization() {
        let capture = Capture::new(
            "Test content".to_string(),
            CaptureType::Clipboard,
            Some("App".to_string()),
        );

        let json = serde_json::to_string(&capture).unwrap();
        let deserialized: Capture = serde_json::from_str(&json).unwrap();

        assert_eq!(capture.content, deserialized.content);
        assert_eq!(capture.capture_type, deserialized.capture_type);
        assert_eq!(capture.source_app, deserialized.source_app);
        assert_eq!(capture.content_hash, deserialized.content_hash);
    }
}
