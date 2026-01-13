//! Privacy filtering for captured content.
//!
//! This module provides comprehensive privacy protection for captured text:
//!
//! - **Pattern-based filtering**: Detects sensitive data like API keys, passwords,
//!   credit card numbers, and SSNs using configurable regex patterns.
//!
//! - **Application exclusion**: Prevents capture from sensitive applications
//!   like password managers.
//!
//! - **Multiple modes**: Block, redact, or warn-only modes for different use cases.
//!
//! # Example
//!
//! ```
//! use flightrecorder::privacy::{PrivacyFilter, FilterResult};
//!
//! let filter = PrivacyFilter::new();
//!
//! // Check if content should be filtered
//! match filter.filter("Some text with api_key=secret123456789abc") {
//!     FilterResult::Passed => println!("Content is safe"),
//!     FilterResult::Blocked { pattern_name } => {
//!         println!("Content blocked by: {}", pattern_name);
//!     }
//!     FilterResult::Redacted { content, .. } => {
//!         println!("Redacted content: {}", content);
//!     }
//! }
//!
//! // Check if an app is excluded
//! if filter.is_app_excluded("1Password") {
//!     println!("Don't capture from this app");
//! }
//! ```

mod filter;
mod patterns;

pub use filter::{FilterConfig, FilterMode, FilterResult, PrivacyFilter};
pub use patterns::{builtin_patterns, default_excluded_apps, FilterPattern};
