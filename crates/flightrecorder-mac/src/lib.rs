//! macOS-specific implementation for flightrecorder
//!
//! This crate provides macOS-specific functionality for the flightrecorder project.

#![cfg(target_os = "macos")]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

/// Initialize macOS-specific components
///
/// # Errors
///
/// Returns an error if initialization fails
pub fn init() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

/// Get platform name
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
}
