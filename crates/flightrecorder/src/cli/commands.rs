//! CLI command definitions.
//!
//! This module defines the structure of all CLI subcommands.

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

/// Daemon management commands.
#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the capture daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Stop the running daemon
    Stop {
        /// Force stop even if daemon is unresponsive
        #[arg(short, long)]
        force: bool,
    },

    /// Restart the daemon
    Restart {
        /// Run in foreground after restart
        #[arg(short, long)]
        foreground: bool,
    },

    /// Install daemon as system service
    Install {
        /// Start the service after installation
        #[arg(long)]
        start: bool,
    },

    /// Uninstall daemon system service
    Uninstall {
        /// Stop the service before uninstalling
        #[arg(long)]
        stop: bool,
    },
}

/// Status command arguments.
#[derive(Debug, Args)]
pub struct StatusCommand {
    /// Output as JSON
    #[arg(short, long)]
    pub json: bool,
}

/// Search command arguments.
#[derive(Debug, Args)]
pub struct SearchCommand {
    /// The search query (searches content)
    pub query: String,

    /// Filter by source application
    #[arg(short, long)]
    pub app: Option<String>,

    /// Filter by capture type
    #[arg(short = 't', long, value_enum)]
    pub capture_type: Option<CaptureTypeArg>,

    /// Show captures since this time (e.g., "1 hour ago", "2024-01-15")
    #[arg(long)]
    pub since: Option<String>,

    /// Show captures until this time
    #[arg(long)]
    pub until: Option<String>,

    /// Maximum number of results
    #[arg(short, long, default_value = "20")]
    pub limit: usize,

    /// Output format
    #[arg(short, long, value_enum, default_value = "table")]
    pub format: OutputFormat,
}

/// Recover command arguments.
#[derive(Debug, Args)]
pub struct RecoverCommand {
    /// Show the last N captures
    #[arg(short, long)]
    pub last: Option<usize>,

    /// Filter by source application
    #[arg(short, long)]
    pub app: Option<String>,

    /// Show captures since this time
    #[arg(long)]
    pub since: Option<String>,

    /// Copy recovered content to clipboard
    #[arg(long)]
    pub to_clipboard: bool,

    /// Launch interactive TUI for recovery
    #[arg(short, long)]
    pub interactive: bool,

    /// Output format
    #[arg(short, long, value_enum, default_value = "plain")]
    pub format: OutputFormat,
}

/// Configuration commands.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Show current configuration
    Show {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,
    },

    /// Show the configuration file path
    Path,

    /// Edit configuration file in default editor
    Edit,

    /// Reset configuration to defaults
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Validate configuration
    Validate {
        /// Path to configuration file to validate
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
}

/// Capture type argument for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CaptureTypeArg {
    /// Clipboard captures
    Clipboard,
    /// Text field captures
    TextField,
    /// Keystroke captures
    Keystroke,
}

impl From<CaptureTypeArg> for crate::capture::CaptureType {
    fn from(arg: CaptureTypeArg) -> Self {
        match arg {
            CaptureTypeArg::Clipboard => Self::Clipboard,
            CaptureTypeArg::TextField => Self::TextField,
            CaptureTypeArg::Keystroke => Self::Keystroke,
        }
    }
}

/// Output format for commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum OutputFormat {
    /// Plain text output
    #[default]
    Plain,
    /// Formatted table
    Table,
    /// JSON output
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_type_arg_conversion() {
        assert_eq!(
            crate::capture::CaptureType::from(CaptureTypeArg::Clipboard),
            crate::capture::CaptureType::Clipboard
        );
        assert_eq!(
            crate::capture::CaptureType::from(CaptureTypeArg::TextField),
            crate::capture::CaptureType::TextField
        );
        assert_eq!(
            crate::capture::CaptureType::from(CaptureTypeArg::Keystroke),
            crate::capture::CaptureType::Keystroke
        );
    }

    #[test]
    fn test_output_format_default() {
        assert_eq!(OutputFormat::default(), OutputFormat::Plain);
    }

    #[test]
    fn test_daemon_command_debug() {
        let cmd = DaemonCommand::Start { foreground: true };
        let debug_str = format!("{cmd:?}");
        assert!(debug_str.contains("Start"));
        assert!(debug_str.contains("foreground"));
    }

    #[test]
    fn test_status_command_debug() {
        let cmd = StatusCommand { json: true };
        let debug_str = format!("{cmd:?}");
        assert!(debug_str.contains("json"));
    }

    #[test]
    fn test_search_command_debug() {
        let cmd = SearchCommand {
            query: "test".to_string(),
            app: None,
            capture_type: None,
            since: None,
            until: None,
            limit: 20,
            format: OutputFormat::Table,
        };
        let debug_str = format!("{cmd:?}");
        assert!(debug_str.contains("query"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_recover_command_debug() {
        let cmd = RecoverCommand {
            last: Some(10),
            app: None,
            since: None,
            to_clipboard: false,
            interactive: false,
            format: OutputFormat::Plain,
        };
        let debug_str = format!("{cmd:?}");
        assert!(debug_str.contains("last"));
    }

    #[test]
    fn test_config_command_debug() {
        let cmd = ConfigCommand::Show { json: false };
        let debug_str = format!("{cmd:?}");
        assert!(debug_str.contains("Show"));
    }

    #[test]
    fn test_capture_type_arg_debug() {
        let arg = CaptureTypeArg::Clipboard;
        let debug_str = format!("{arg:?}");
        assert_eq!(debug_str, "Clipboard");
    }

    #[test]
    fn test_output_format_debug() {
        let format = OutputFormat::Json;
        let debug_str = format!("{format:?}");
        assert_eq!(debug_str, "Json");
    }

    #[test]
    fn test_capture_type_arg_clone() {
        let arg = CaptureTypeArg::TextField;
        let cloned = arg;
        assert_eq!(arg, cloned);
    }

    #[test]
    fn test_output_format_clone() {
        let format = OutputFormat::Table;
        let cloned = format;
        assert_eq!(format, cloned);
    }
}
