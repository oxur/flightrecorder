//! Configuration management for flightrecorder.
//!
//! This module provides configuration loading and validation using figment,
//! supporting TOML config files, environment variables, and defaults.

use std::path::PathBuf;
use std::time::Duration;

use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Default configuration file name.
const CONFIG_FILE_NAME: &str = "config.toml";

/// Default data directory name.
const DATA_DIR_NAME: &str = "flightrecorder";

/// Default database file name.
const DATABASE_FILE_NAME: &str = "captures.db";

/// Application configuration.
///
/// Configuration is loaded from (in order of precedence, highest first):
/// 1. Environment variables (prefixed with `FLIGHTRECORDER_`)
/// 2. TOML config file at `~/.config/flightrecorder/config.toml`
/// 3. Default values
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Storage configuration.
    pub storage: StorageConfig,
    /// Capture configuration.
    pub capture: CaptureConfig,
    /// Privacy configuration.
    pub privacy: PrivacyConfig,
    /// Daemon configuration.
    pub daemon: DaemonConfig,
}

/// Storage-related configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Path to the database file.
    /// Defaults to `~/.local/share/flightrecorder/captures.db`
    pub database_path: Option<PathBuf>,
    /// Maximum number of captures to retain.
    /// Set to 0 for unlimited.
    pub max_captures: usize,
    /// Maximum age of captures to retain in days.
    /// Set to 0 for unlimited.
    pub max_age_days: u32,
    /// Prune interval in hours.
    pub prune_interval_hours: u32,
}

/// Capture-related configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    /// Enable clipboard monitoring.
    pub clipboard_enabled: bool,
    /// Enable accessibility text field capture.
    pub accessibility_enabled: bool,
    /// Enable keystroke fallback (Linux Wayland only).
    pub keystroke_fallback_enabled: bool,
    /// Interval between text field snapshots in milliseconds.
    pub snapshot_interval_ms: u64,
    /// Minimum content length to capture.
    pub min_content_length: usize,
    /// Maximum content length to capture.
    pub max_content_length: usize,
}

/// Privacy-related configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    /// Enable built-in privacy filters.
    pub filters_enabled: bool,
    /// Filter patterns for sensitive content (regex).
    pub filter_patterns: Vec<String>,
    /// Applications to exclude from capture.
    pub excluded_apps: Vec<String>,
    /// Skip password fields detected via accessibility.
    pub skip_password_fields: bool,
}

/// Daemon-related configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    /// Path to the Unix socket for IPC.
    /// Defaults to `~/.local/share/flightrecorder/fliterec.sock`
    pub socket_path: Option<PathBuf>,
    /// Path to the PID file.
    /// Defaults to `~/.local/share/flightrecorder/fliterec.pid`
    pub pid_file_path: Option<PathBuf>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database_path: None, // Will be resolved to default at runtime
            max_captures: 100_000,
            max_age_days: 30,
            prune_interval_hours: 24,
        }
    }
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            clipboard_enabled: true,
            accessibility_enabled: true,
            keystroke_fallback_enabled: false, // Opt-in only
            snapshot_interval_ms: 500,
            min_content_length: 1,
            max_content_length: 1_000_000, // 1MB max
        }
    }
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            filters_enabled: true,
            filter_patterns: default_filter_patterns(),
            excluded_apps: default_excluded_apps(),
            skip_password_fields: true,
        }
    }
}

/// Default filter patterns for sensitive content.
fn default_filter_patterns() -> Vec<String> {
    vec![
        // API keys and tokens
        r"(?i)(api[_-]?key|api[_-]?secret|access[_-]?token)\s*[:=]\s*\S+".to_string(),
        // AWS credentials
        r"(?i)(aws[_-]?access[_-]?key|aws[_-]?secret)[\s:=]+\S+".to_string(),
        // Generic passwords
        r"(?i)(password|passwd|pwd)\s*[:=]\s*\S+".to_string(),
        // Credit card numbers (basic pattern)
        r"\b(?:\d{4}[- ]?){3}\d{4}\b".to_string(),
        // SSN pattern
        r"\b\d{3}-\d{2}-\d{4}\b".to_string(),
    ]
}

/// Default applications to exclude from capture.
fn default_excluded_apps() -> Vec<String> {
    vec![
        "1Password".to_string(),
        "Bitwarden".to_string(),
        "LastPass".to_string(),
        "KeePassXC".to_string(),
        "Keychain Access".to_string(),
        "Dashlane".to_string(),
        "Enpass".to_string(),
    ]
}

impl Config {
    /// Load configuration from all sources.
    ///
    /// Configuration is loaded in this order (later sources override earlier):
    /// 1. Default values
    /// 2. TOML config file (if exists)
    /// 3. Environment variables (prefixed with `FLIGHTRECORDER_`)
    ///
    /// # Errors
    ///
    /// Returns an error if configuration loading or parsing fails.
    pub fn load() -> Result<Self> {
        Self::load_from(None)
    }

    /// Load configuration with an optional custom config path.
    ///
    /// # Errors
    ///
    /// Returns an error if configuration loading or parsing fails.
    pub fn load_from(config_path: Option<PathBuf>) -> Result<Self> {
        let config_file = config_path.unwrap_or_else(Self::default_config_path);

        let figment = Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file(&config_file).nested())
            .merge(Env::prefixed("FLIGHTRECORDER_").split("_"));

        let config: Config = figment.extract()?;
        config.validate()?;
        Ok(config)
    }

    /// Get the default configuration file path.
    #[must_use]
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join(DATA_DIR_NAME)
            .join(CONFIG_FILE_NAME)
    }

    /// Get the default data directory path.
    #[must_use]
    pub fn default_data_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from(".local/share"))
            .join(DATA_DIR_NAME)
    }

    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any configuration values are invalid.
    pub fn validate(&self) -> Result<()> {
        // Validate capture config
        if self.capture.min_content_length > self.capture.max_content_length {
            return Err(Error::ConfigValidation {
                message: format!(
                    "min_content_length ({}) cannot be greater than max_content_length ({})",
                    self.capture.min_content_length, self.capture.max_content_length
                ),
            });
        }

        if self.capture.snapshot_interval_ms == 0 {
            return Err(Error::ConfigValidation {
                message: "snapshot_interval_ms must be greater than 0".to_string(),
            });
        }

        // Validate regex patterns
        for pattern in &self.privacy.filter_patterns {
            if regex::Regex::new(pattern).is_err() {
                return Err(Error::ConfigValidation {
                    message: format!("invalid regex pattern: {pattern}"),
                });
            }
        }

        Ok(())
    }

    /// Get the database path, resolving defaults if not set.
    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        self.storage
            .database_path
            .clone()
            .unwrap_or_else(|| Self::default_data_dir().join(DATABASE_FILE_NAME))
    }

    /// Get the socket path, resolving defaults if not set.
    #[must_use]
    pub fn socket_path(&self) -> PathBuf {
        self.daemon
            .socket_path
            .clone()
            .unwrap_or_else(|| Self::default_data_dir().join("fliterec.sock"))
    }

    /// Get the PID file path, resolving defaults if not set.
    #[must_use]
    pub fn pid_file_path(&self) -> PathBuf {
        self.daemon
            .pid_file_path
            .clone()
            .unwrap_or_else(|| Self::default_data_dir().join("fliterec.pid"))
    }

    /// Get the max age as a Duration.
    #[must_use]
    pub fn max_age(&self) -> Option<Duration> {
        if self.storage.max_age_days == 0 {
            None
        } else {
            Some(Duration::from_secs(
                u64::from(self.storage.max_age_days) * 24 * 60 * 60,
            ))
        }
    }

    /// Get the prune interval as a Duration.
    #[must_use]
    pub fn prune_interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.storage.prune_interval_hours) * 60 * 60)
    }

    /// Get the snapshot interval as a Duration.
    #[must_use]
    pub fn snapshot_interval(&self) -> Duration {
        Duration::from_millis(self.capture.snapshot_interval_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();

        assert!(config.capture.clipboard_enabled);
        assert!(config.capture.accessibility_enabled);
        assert!(!config.capture.keystroke_fallback_enabled);
        assert!(config.privacy.filters_enabled);
        assert!(config.privacy.skip_password_fields);
    }

    #[test]
    fn test_default_storage_config() {
        let storage = StorageConfig::default();

        assert!(storage.database_path.is_none());
        assert_eq!(storage.max_captures, 100_000);
        assert_eq!(storage.max_age_days, 30);
        assert_eq!(storage.prune_interval_hours, 24);
    }

    #[test]
    fn test_default_capture_config() {
        let capture = CaptureConfig::default();

        assert!(capture.clipboard_enabled);
        assert!(capture.accessibility_enabled);
        assert!(!capture.keystroke_fallback_enabled);
        assert_eq!(capture.snapshot_interval_ms, 500);
        assert_eq!(capture.min_content_length, 1);
        assert_eq!(capture.max_content_length, 1_000_000);
    }

    #[test]
    fn test_default_privacy_config() {
        let privacy = PrivacyConfig::default();

        assert!(privacy.filters_enabled);
        assert!(privacy.skip_password_fields);
        assert!(!privacy.filter_patterns.is_empty());
        assert!(!privacy.excluded_apps.is_empty());
    }

    #[test]
    fn test_default_daemon_config() {
        let daemon = DaemonConfig::default();

        assert!(daemon.socket_path.is_none());
        assert!(daemon.pid_file_path.is_none());
    }

    #[test]
    fn test_validate_valid_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_content_length() {
        let mut config = Config::default();
        config.capture.min_content_length = 1000;
        config.capture.max_content_length = 100;

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("min_content_length"));
    }

    #[test]
    fn test_validate_zero_snapshot_interval() {
        let mut config = Config::default();
        config.capture.snapshot_interval_ms = 0;

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("snapshot_interval_ms"));
    }

    #[test]
    fn test_validate_invalid_regex() {
        let mut config = Config::default();
        config.privacy.filter_patterns = vec!["[invalid".to_string()];

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid regex"));
    }

    #[test]
    fn test_database_path_default() {
        let config = Config::default();
        let path = config.database_path();

        assert!(path.to_string_lossy().contains("captures.db"));
    }

    #[test]
    fn test_database_path_custom() {
        let mut config = Config::default();
        config.storage.database_path = Some(PathBuf::from("/custom/path/db.sqlite"));

        assert_eq!(
            config.database_path(),
            PathBuf::from("/custom/path/db.sqlite")
        );
    }

    #[test]
    fn test_socket_path_default() {
        let config = Config::default();
        let path = config.socket_path();

        assert!(path.to_string_lossy().contains("fliterec.sock"));
    }

    #[test]
    fn test_pid_file_path_default() {
        let config = Config::default();
        let path = config.pid_file_path();

        assert!(path.to_string_lossy().contains("fliterec.pid"));
    }

    #[test]
    fn test_max_age_none_when_zero() {
        let mut config = Config::default();
        config.storage.max_age_days = 0;

        assert!(config.max_age().is_none());
    }

    #[test]
    fn test_max_age_some_when_set() {
        let config = Config::default();
        let max_age = config.max_age();

        assert!(max_age.is_some());
        assert_eq!(max_age.unwrap(), Duration::from_secs(30 * 24 * 60 * 60));
    }

    #[test]
    fn test_prune_interval() {
        let config = Config::default();
        let interval = config.prune_interval();

        assert_eq!(interval, Duration::from_secs(24 * 60 * 60));
    }

    #[test]
    fn test_snapshot_interval() {
        let config = Config::default();
        let interval = config.snapshot_interval();

        assert_eq!(interval, Duration::from_millis(500));
    }

    #[test]
    fn test_default_filter_patterns_are_valid() {
        let patterns = default_filter_patterns();
        for pattern in patterns {
            assert!(
                regex::Regex::new(&pattern).is_ok(),
                "Invalid pattern: {pattern}"
            );
        }
    }

    #[test]
    fn test_default_excluded_apps_not_empty() {
        let apps = default_excluded_apps();
        assert!(!apps.is_empty());
        assert!(apps.contains(&"1Password".to_string()));
    }

    #[test]
    fn test_config_debug() {
        let config = Config::default();
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("Config"));
    }

    #[test]
    fn test_config_clone() {
        let config = Config::default();
        let cloned = config.clone();
        assert_eq!(config, cloned);
    }

    #[test]
    fn test_default_config_path() {
        let path = Config::default_config_path();
        assert!(path.to_string_lossy().contains("flightrecorder"));
        assert!(path.to_string_lossy().contains("config.toml"));
    }

    #[test]
    fn test_default_data_dir() {
        let path = Config::default_data_dir();
        assert!(path.to_string_lossy().contains("flightrecorder"));
    }

    #[test]
    fn test_load_nonexistent_config() {
        // Loading from a nonexistent path should work (uses defaults)
        let result = Config::load_from(Some(PathBuf::from("/nonexistent/config.toml")));
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_storage_config_serialize() {
        let storage = StorageConfig::default();
        let json = serde_json::to_string(&storage).unwrap();
        assert!(json.contains("max_captures"));
    }

    #[test]
    fn test_storage_config_deserialize() {
        let json = r#"{"max_captures": 5000, "max_age_days": 7}"#;
        let storage: StorageConfig = serde_json::from_str(json).unwrap();
        assert_eq!(storage.max_captures, 5000);
        assert_eq!(storage.max_age_days, 7);
    }

    #[test]
    fn test_capture_config_serialize() {
        let capture = CaptureConfig::default();
        let json = serde_json::to_string(&capture).unwrap();
        assert!(json.contains("clipboard_enabled"));
    }

    #[test]
    fn test_privacy_config_serialize() {
        let privacy = PrivacyConfig::default();
        let json = serde_json::to_string(&privacy).unwrap();
        assert!(json.contains("filters_enabled"));
    }

    #[test]
    fn test_daemon_config_serialize() {
        let daemon = DaemonConfig::default();
        let json = serde_json::to_string(&daemon).unwrap();
        assert!(json.contains("socket_path"));
    }
}
