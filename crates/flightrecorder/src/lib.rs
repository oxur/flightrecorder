//! `flightrecorder` - A system-level service that preserves ephemeral text input
//!
//! This library provides the core functionality for capturing and storing text input
//! from clipboard operations and accessibility-based text field monitoring.

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

pub mod capture;
pub mod config;
pub mod error;
pub mod logging;
pub mod storage;

pub use capture::{Capture, CaptureSource, CaptureType};
pub use config::Config;
pub use error::{Error, Result};
pub use logging::init_logging;
pub use storage::{Storage, StorageStats};
