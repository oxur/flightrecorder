//! Built-in privacy filter patterns.
//!
//! This module provides pre-defined regex patterns for detecting sensitive
//! information that should not be captured or stored.

use regex::Regex;

/// A compiled privacy filter pattern.
#[derive(Debug)]
pub struct FilterPattern {
    /// Name of the pattern for identification.
    pub name: &'static str,

    /// Description of what this pattern matches.
    pub description: &'static str,

    /// The compiled regex.
    regex: Regex,
}

impl FilterPattern {
    /// Create a new filter pattern.
    ///
    /// # Panics
    ///
    /// Panics if the regex pattern is invalid.
    #[must_use]
    pub fn new(name: &'static str, description: &'static str, pattern: &str) -> Self {
        Self {
            name,
            description,
            regex: Regex::new(pattern).expect("Invalid regex pattern"),
        }
    }

    /// Check if the content matches this pattern.
    #[must_use]
    pub fn matches(&self, content: &str) -> bool {
        self.regex.is_match(content)
    }

    /// Find all matches in the content.
    pub fn find_all<'a>(
        &self,
        content: &'a str,
    ) -> impl Iterator<Item = regex::Match<'a>> + use<'a, '_> {
        self.regex.find_iter(content)
    }

    /// Redact matches in the content, replacing them with a placeholder.
    #[must_use]
    pub fn redact(&self, content: &str, placeholder: &str) -> String {
        self.regex.replace_all(content, placeholder).to_string()
    }
}

/// Get all built-in filter patterns.
#[must_use]
pub fn builtin_patterns() -> Vec<FilterPattern> {
    vec![
        // API Keys
        FilterPattern::new(
            "api_key_generic",
            "Generic API key patterns (api_key=, apikey=, etc.)",
            r#"(?i)(api[_-]?key|apikey|api[_-]?secret)\s*[:=]\s*['"]?[a-zA-Z0-9_-]{16,}['"]?"#,
        ),
        FilterPattern::new(
            "bearer_token",
            "Bearer authentication tokens",
            r"(?i)bearer\s+[a-zA-Z0-9_.=-]+",
        ),
        FilterPattern::new(
            "aws_key",
            "AWS access key IDs",
            r"(?i)(AKIA|ABIA|ACCA|ASIA)[A-Z0-9]{16}",
        ),
        FilterPattern::new(
            "aws_secret",
            "AWS secret access keys",
            r#"(?i)aws[_-]?secret[_-]?access[_-]?key\s*[:=]\s*['"]?[A-Za-z0-9/+=]{40}['"]?"#,
        ),
        // Passwords
        FilterPattern::new(
            "password_field",
            "Password field assignments",
            r#"(?i)(password|passwd|pwd|secret)\s*[:=]\s*['"]?[^\s'"]{4,}['"]?"#,
        ),
        // Credit Cards
        FilterPattern::new(
            "credit_card",
            "Credit card numbers (Visa, MasterCard, Amex, Discover)",
            r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b",
        ),
        // SSN
        FilterPattern::new(
            "ssn",
            "US Social Security Numbers",
            r"\b\d{3}-\d{2}-\d{4}\b",
        ),
        // Private Keys
        FilterPattern::new(
            "private_key",
            "PEM-encoded private keys",
            r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----",
        ),
        // GitHub Tokens
        FilterPattern::new(
            "github_token",
            "GitHub personal access tokens and app tokens",
            r"(?i)(ghp_|gho_|ghu_|ghs_|ghr_)[a-zA-Z0-9]{36}",
        ),
        // Slack Tokens
        FilterPattern::new(
            "slack_token",
            "Slack API tokens",
            r"xox[baprs]-[0-9]+-[0-9]+-[a-zA-Z0-9]+",
        ),
        // Generic secrets
        FilterPattern::new(
            "connection_string",
            "Database connection strings with credentials",
            r"(?i)(mongodb|postgres|mysql|redis)://[^:]+:[^@]+@",
        ),
    ]
}

/// Default list of applications to exclude from capture.
#[must_use]
pub fn default_excluded_apps() -> Vec<&'static str> {
    vec![
        // Password Managers
        "1Password",
        "1Password 7",
        "1Password 8",
        "Bitwarden",
        "LastPass",
        "Dashlane",
        "KeePassXC",
        "Keychain Access",
        // Security Tools
        "Authenticator",
        "Google Authenticator",
        "Microsoft Authenticator",
        "Authy",
        // Sensitive System Apps
        "Terminal", // Often contains credentials
        "iTerm2",
        "Alacritty",
        "Warp",
        // Browsers (in private mode detection is hard)
        // We don't exclude browsers by default as they're common use cases
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_pattern_matches_api_key() {
        let pattern = FilterPattern::new(
            "test",
            "Test pattern",
            r#"(?i)api[_-]?key\s*[:=]\s*['"]?[a-zA-Z0-9_-]{16,}['"]?"#,
        );

        assert!(pattern.matches("api_key=abc123def456ghi789"));
        assert!(pattern.matches("API_KEY = 'abc123def456ghi789'"));
        assert!(pattern.matches("apiKey: abc123def456ghi789jkl"));
        assert!(!pattern.matches("api_key=short"));
        assert!(!pattern.matches("regular text without keys"));
    }

    #[test]
    fn test_filter_pattern_matches_credit_card() {
        let pattern = FilterPattern::new(
            "cc",
            "Credit cards",
            r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b",
        );

        // Visa
        assert!(pattern.matches("4111111111111111"));
        // MasterCard
        assert!(pattern.matches("5500000000000004"));
        // Amex
        assert!(pattern.matches("340000000000009"));
        // Not a card
        assert!(!pattern.matches("1234567890123456"));
        assert!(!pattern.matches("regular text"));
    }

    #[test]
    fn test_filter_pattern_matches_ssn() {
        let pattern = FilterPattern::new("ssn", "SSN", r"\b\d{3}-\d{2}-\d{4}\b");

        assert!(pattern.matches("123-45-6789"));
        assert!(pattern.matches("My SSN is 123-45-6789 okay"));
        assert!(!pattern.matches("123456789"));
        assert!(!pattern.matches("123-456-789"));
    }

    #[test]
    fn test_filter_pattern_redact() {
        let pattern = FilterPattern::new("ssn", "SSN", r"\b\d{3}-\d{2}-\d{4}\b");

        let content = "My SSN is 123-45-6789 and yours is 987-65-4321";
        let redacted = pattern.redact(content, "[REDACTED]");

        assert_eq!(redacted, "My SSN is [REDACTED] and yours is [REDACTED]");
    }

    #[test]
    fn test_filter_pattern_find_all() {
        let pattern = FilterPattern::new("ssn", "SSN", r"\b\d{3}-\d{2}-\d{4}\b");

        let content = "SSN1: 123-45-6789, SSN2: 987-65-4321";
        let matches: Vec<_> = pattern.find_all(content).collect();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].as_str(), "123-45-6789");
        assert_eq!(matches[1].as_str(), "987-65-4321");
    }

    #[test]
    fn test_builtin_patterns_not_empty() {
        let patterns = builtin_patterns();
        assert!(!patterns.is_empty());
        assert!(patterns.len() >= 10);
    }

    #[test]
    fn test_builtin_patterns_have_names() {
        let patterns = builtin_patterns();
        for pattern in patterns {
            assert!(!pattern.name.is_empty());
            assert!(!pattern.description.is_empty());
        }
    }

    #[test]
    fn test_default_excluded_apps_not_empty() {
        let apps = default_excluded_apps();
        assert!(!apps.is_empty());
        assert!(apps.contains(&"1Password"));
        assert!(apps.contains(&"Bitwarden"));
    }

    #[test]
    fn test_aws_key_pattern() {
        let patterns = builtin_patterns();
        let aws_pattern = patterns.iter().find(|p| p.name == "aws_key").unwrap();

        assert!(aws_pattern.matches("AKIAIOSFODNN7EXAMPLE"));
        assert!(!aws_pattern.matches("not an aws key"));
    }

    #[test]
    fn test_github_token_pattern() {
        let patterns = builtin_patterns();
        let gh_pattern = patterns.iter().find(|p| p.name == "github_token").unwrap();

        assert!(gh_pattern.matches("ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"));
        assert!(gh_pattern.matches("gho_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"));
        assert!(!gh_pattern.matches("not a github token"));
    }

    #[test]
    fn test_private_key_pattern() {
        let patterns = builtin_patterns();
        let key_pattern = patterns.iter().find(|p| p.name == "private_key").unwrap();

        assert!(key_pattern.matches("-----BEGIN PRIVATE KEY-----"));
        assert!(key_pattern.matches("-----BEGIN RSA PRIVATE KEY-----"));
        assert!(key_pattern.matches("-----BEGIN EC PRIVATE KEY-----"));
        assert!(!key_pattern.matches("-----BEGIN PUBLIC KEY-----"));
    }
}
