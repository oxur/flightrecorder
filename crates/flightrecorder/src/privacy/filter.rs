//! Privacy filter for captured content.
//!
//! This module provides the main privacy filter that processes captured
//! content before storage, detecting and filtering sensitive information.

use regex::Regex;
use tracing::{debug, trace};

use super::patterns::{builtin_patterns, FilterPattern};

/// Result of filtering content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterResult {
    /// Content passed all filters and is safe to store.
    Passed,

    /// Content was blocked entirely due to sensitive data.
    Blocked {
        /// Name of the pattern that caused the block.
        pattern_name: String,
    },

    /// Content was redacted (sensitive parts replaced).
    Redacted {
        /// The redacted content.
        content: String,

        /// Patterns that were redacted.
        redacted_patterns: Vec<String>,
    },
}

/// Mode of operation for the privacy filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterMode {
    /// Block content that contains sensitive data.
    #[default]
    Block,

    /// Redact sensitive data from content.
    Redact,

    /// Only log warnings, don't filter.
    WarnOnly,
}

/// Configuration for the privacy filter.
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// Whether filtering is enabled.
    pub enabled: bool,

    /// Filter mode.
    pub mode: FilterMode,

    /// Whether to use built-in patterns.
    pub use_builtin_patterns: bool,

    /// Custom regex patterns to filter.
    pub custom_patterns: Vec<String>,

    /// Applications to exclude from capture.
    pub excluded_apps: Vec<String>,

    /// Placeholder text for redacted content.
    pub redaction_placeholder: String,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: FilterMode::Block,
            use_builtin_patterns: true,
            custom_patterns: Vec::new(),
            excluded_apps: super::patterns::default_excluded_apps()
                .into_iter()
                .map(String::from)
                .collect(),
            redaction_placeholder: "[REDACTED]".to_string(),
        }
    }
}

/// Privacy filter for captured content.
#[derive(Debug)]
pub struct PrivacyFilter {
    config: FilterConfig,
    patterns: Vec<FilterPattern>,
    custom_regexes: Vec<Regex>,
}

impl PrivacyFilter {
    /// Create a new privacy filter with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(FilterConfig::default())
    }

    /// Create a new privacy filter with custom configuration.
    #[must_use]
    pub fn with_config(config: FilterConfig) -> Self {
        let patterns = if config.use_builtin_patterns {
            builtin_patterns()
        } else {
            Vec::new()
        };

        let custom_regexes = config
            .custom_patterns
            .iter()
            .filter_map(|p| match Regex::new(p) {
                Ok(r) => Some(r),
                Err(e) => {
                    tracing::warn!(pattern = %p, error = %e, "Invalid custom regex pattern");
                    None
                }
            })
            .collect();

        Self {
            config,
            patterns,
            custom_regexes,
        }
    }

    /// Check if filtering is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if an application is excluded from capture.
    #[must_use]
    pub fn is_app_excluded(&self, app_name: &str) -> bool {
        self.config
            .excluded_apps
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(app_name))
    }

    /// Filter content and return the result.
    #[must_use]
    pub fn filter(&self, content: &str) -> FilterResult {
        if !self.config.enabled {
            return FilterResult::Passed;
        }

        match self.config.mode {
            FilterMode::Block => self.filter_block(content),
            FilterMode::Redact => self.filter_redact(content),
            FilterMode::WarnOnly => self.filter_warn(content),
        }
    }

    /// Block mode: return Blocked if any pattern matches.
    fn filter_block(&self, content: &str) -> FilterResult {
        // Check built-in patterns
        for pattern in &self.patterns {
            if pattern.matches(content) {
                debug!(pattern = %pattern.name, "Content blocked by pattern");
                return FilterResult::Blocked {
                    pattern_name: pattern.name.to_string(),
                };
            }
        }

        // Check custom patterns
        for (i, regex) in self.custom_regexes.iter().enumerate() {
            if regex.is_match(content) {
                debug!(pattern_index = %i, "Content blocked by custom pattern");
                return FilterResult::Blocked {
                    pattern_name: format!("custom_{i}"),
                };
            }
        }

        FilterResult::Passed
    }

    /// Redact mode: replace matches with placeholder.
    fn filter_redact(&self, content: &str) -> FilterResult {
        let mut result = content.to_string();
        let mut redacted_patterns = Vec::new();

        // Redact built-in patterns
        for pattern in &self.patterns {
            if pattern.matches(&result) {
                result = pattern.redact(&result, &self.config.redaction_placeholder);
                redacted_patterns.push(pattern.name.to_string());
                trace!(pattern = %pattern.name, "Content redacted by pattern");
            }
        }

        // Redact custom patterns
        for (i, regex) in self.custom_regexes.iter().enumerate() {
            if regex.is_match(&result) {
                result = regex
                    .replace_all(&result, &self.config.redaction_placeholder)
                    .to_string();
                redacted_patterns.push(format!("custom_{i}"));
                trace!(pattern_index = %i, "Content redacted by custom pattern");
            }
        }

        if redacted_patterns.is_empty() {
            FilterResult::Passed
        } else {
            FilterResult::Redacted {
                content: result,
                redacted_patterns,
            }
        }
    }

    /// Warn mode: log warnings but pass content through.
    fn filter_warn(&self, content: &str) -> FilterResult {
        // Check and warn about built-in patterns
        for pattern in &self.patterns {
            if pattern.matches(content) {
                tracing::warn!(
                    pattern = %pattern.name,
                    description = %pattern.description,
                    "Sensitive data detected (warn mode)"
                );
            }
        }

        // Check and warn about custom patterns
        for (i, regex) in self.custom_regexes.iter().enumerate() {
            if regex.is_match(content) {
                tracing::warn!(
                    pattern_index = %i,
                    "Sensitive data detected by custom pattern (warn mode)"
                );
            }
        }

        FilterResult::Passed
    }

    /// Get the list of excluded apps.
    #[must_use]
    pub fn excluded_apps(&self) -> &[String] {
        &self.config.excluded_apps
    }

    /// Add an application to the exclusion list.
    pub fn exclude_app(&mut self, app_name: &str) {
        if !self.is_app_excluded(app_name) {
            self.config.excluded_apps.push(app_name.to_string());
        }
    }

    /// Remove an application from the exclusion list.
    pub fn unexclude_app(&mut self, app_name: &str) {
        self.config
            .excluded_apps
            .retain(|a| !a.eq_ignore_ascii_case(app_name));
    }
}

impl Default for PrivacyFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_result_passed() {
        let result = FilterResult::Passed;
        assert!(matches!(result, FilterResult::Passed));
    }

    #[test]
    fn test_filter_result_blocked() {
        let result = FilterResult::Blocked {
            pattern_name: "test".to_string(),
        };
        if let FilterResult::Blocked { pattern_name } = result {
            assert_eq!(pattern_name, "test");
        } else {
            panic!("Expected Blocked result");
        }
    }

    #[test]
    fn test_filter_result_redacted() {
        let result = FilterResult::Redacted {
            content: "test [REDACTED]".to_string(),
            redacted_patterns: vec!["pattern1".to_string()],
        };
        if let FilterResult::Redacted {
            content,
            redacted_patterns,
        } = result
        {
            assert_eq!(content, "test [REDACTED]");
            assert_eq!(redacted_patterns.len(), 1);
        } else {
            panic!("Expected Redacted result");
        }
    }

    #[test]
    fn test_filter_mode_default() {
        assert_eq!(FilterMode::default(), FilterMode::Block);
    }

    #[test]
    fn test_filter_config_default() {
        let config = FilterConfig::default();
        assert!(config.enabled);
        assert_eq!(config.mode, FilterMode::Block);
        assert!(config.use_builtin_patterns);
        assert!(config.custom_patterns.is_empty());
        assert!(!config.excluded_apps.is_empty());
        assert_eq!(config.redaction_placeholder, "[REDACTED]");
    }

    #[test]
    fn test_privacy_filter_new() {
        let filter = PrivacyFilter::new();
        assert!(filter.is_enabled());
        assert!(!filter.patterns.is_empty());
    }

    #[test]
    fn test_privacy_filter_default() {
        let filter = PrivacyFilter::default();
        assert!(filter.is_enabled());
    }

    #[test]
    fn test_filter_disabled() {
        let config = FilterConfig {
            enabled: false,
            ..Default::default()
        };
        let filter = PrivacyFilter::with_config(config);

        assert!(!filter.is_enabled());
        let result = filter.filter("api_key=abc123def456ghi789jkl");
        assert!(matches!(result, FilterResult::Passed));
    }

    #[test]
    fn test_filter_block_mode_blocks_sensitive() {
        let filter = PrivacyFilter::new();

        let result = filter.filter("my api_key=abcdef1234567890ghij");
        assert!(matches!(result, FilterResult::Blocked { .. }));
    }

    #[test]
    fn test_filter_block_mode_passes_safe() {
        let filter = PrivacyFilter::new();

        let result = filter.filter("This is just regular text content.");
        assert!(matches!(result, FilterResult::Passed));
    }

    #[test]
    fn test_filter_redact_mode() {
        let config = FilterConfig {
            mode: FilterMode::Redact,
            ..Default::default()
        };
        let filter = PrivacyFilter::with_config(config);

        let result = filter.filter("SSN: 123-45-6789 is secret");
        if let FilterResult::Redacted {
            content,
            redacted_patterns,
        } = result
        {
            assert!(content.contains("[REDACTED]"));
            assert!(!content.contains("123-45-6789"));
            assert!(!redacted_patterns.is_empty());
        } else {
            panic!("Expected Redacted result");
        }
    }

    #[test]
    fn test_filter_warn_mode_passes() {
        let config = FilterConfig {
            mode: FilterMode::WarnOnly,
            ..Default::default()
        };
        let filter = PrivacyFilter::with_config(config);

        let result = filter.filter("api_key=abcdef1234567890ghij");
        // Warn mode always passes
        assert!(matches!(result, FilterResult::Passed));
    }

    #[test]
    fn test_is_app_excluded() {
        let filter = PrivacyFilter::new();

        assert!(filter.is_app_excluded("1Password"));
        assert!(filter.is_app_excluded("1password")); // Case insensitive
        assert!(filter.is_app_excluded("Bitwarden"));
        assert!(!filter.is_app_excluded("Safari"));
        assert!(!filter.is_app_excluded("NotAPasswordManager"));
    }

    #[test]
    fn test_exclude_app() {
        let mut filter = PrivacyFilter::new();

        assert!(!filter.is_app_excluded("MyCustomApp"));
        filter.exclude_app("MyCustomApp");
        assert!(filter.is_app_excluded("MyCustomApp"));
    }

    #[test]
    fn test_unexclude_app() {
        let mut filter = PrivacyFilter::new();

        assert!(filter.is_app_excluded("1Password"));
        filter.unexclude_app("1Password");
        assert!(!filter.is_app_excluded("1Password"));
    }

    #[test]
    fn test_exclude_app_no_duplicates() {
        let mut filter = PrivacyFilter::new();
        let initial_count = filter.excluded_apps().len();

        filter.exclude_app("1Password"); // Already excluded
        assert_eq!(filter.excluded_apps().len(), initial_count);
    }

    #[test]
    fn test_custom_patterns() {
        let config = FilterConfig {
            custom_patterns: vec![r"\bSECRET_CODE_\d+\b".to_string()],
            ..Default::default()
        };
        let filter = PrivacyFilter::with_config(config);

        let result = filter.filter("The SECRET_CODE_12345 is here");
        assert!(matches!(result, FilterResult::Blocked { .. }));
    }

    #[test]
    fn test_custom_patterns_invalid_regex() {
        let config = FilterConfig {
            custom_patterns: vec![
                r"\bvalid\b".to_string(),
                r"[invalid".to_string(), // Invalid regex
            ],
            ..Default::default()
        };
        let filter = PrivacyFilter::with_config(config);

        // Should only have one custom regex (the valid one)
        assert_eq!(filter.custom_regexes.len(), 1);
    }

    #[test]
    fn test_filter_blocks_credit_cards() {
        let filter = PrivacyFilter::new();

        let result = filter.filter("Card: 4111111111111111");
        assert!(matches!(result, FilterResult::Blocked { .. }));
    }

    #[test]
    fn test_filter_blocks_ssn() {
        let filter = PrivacyFilter::new();

        let result = filter.filter("SSN: 123-45-6789");
        assert!(matches!(result, FilterResult::Blocked { .. }));
    }

    #[test]
    fn test_filter_blocks_aws_keys() {
        let filter = PrivacyFilter::new();

        let result = filter.filter("AKIAIOSFODNN7EXAMPLE");
        assert!(matches!(result, FilterResult::Blocked { .. }));
    }

    #[test]
    fn test_filter_blocks_private_keys() {
        let filter = PrivacyFilter::new();

        let result = filter.filter("-----BEGIN RSA PRIVATE KEY-----");
        assert!(matches!(result, FilterResult::Blocked { .. }));
    }

    #[test]
    fn test_excluded_apps_list() {
        let filter = PrivacyFilter::new();
        let apps = filter.excluded_apps();

        assert!(!apps.is_empty());
        // Check some expected apps are present
        assert!(apps.iter().any(|a| a.contains("1Password")));
    }

    #[test]
    fn test_without_builtin_patterns() {
        let config = FilterConfig {
            use_builtin_patterns: false,
            ..Default::default()
        };
        let filter = PrivacyFilter::with_config(config);

        // Without builtin patterns, sensitive content should pass
        let result = filter.filter("api_key=abcdef1234567890ghij");
        assert!(matches!(result, FilterResult::Passed));
    }
}
