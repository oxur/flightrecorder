//! Command-line interface for flightrecorder.
//!
//! This module provides the CLI structure and command handlers for the
//! `fliterec` binary.

mod commands;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub use commands::{ConfigCommand, DaemonCommand, RecoverCommand, SearchCommand, StatusCommand};

/// fliterec - Preserve your ephemeral text input
///
/// A system-level service that captures clipboard changes and text field
/// contents, protecting you from data loss due to crashes and network failures.
#[derive(Debug, Parser)]
#[command(name = "fliterec")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Path to custom configuration file
    #[arg(short, long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Increase verbosity (-v for debug, -vv for trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// The command to execute
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage the capture daemon
    #[command(subcommand)]
    Daemon(DaemonCommand),

    /// Show daemon and capture status
    Status(StatusCommand),

    /// Search captured text
    Search(SearchCommand),

    /// Recover captured text
    Recover(RecoverCommand),

    /// View or modify configuration
    #[command(subcommand)]
    Config(ConfigCommand),
}

impl Cli {
    /// Get the verbosity level based on flags.
    #[must_use]
    pub fn verbosity(&self) -> crate::logging::Verbosity {
        if self.quiet {
            crate::logging::Verbosity::Quiet
        } else {
            match self.verbose {
                0 => crate::logging::Verbosity::Normal,
                1 => crate::logging::Verbosity::Verbose,
                _ => crate::logging::Verbosity::Trace,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_debug() {
        let cli = Cli::command();
        assert_eq!(cli.get_name(), "fliterec");
    }

    #[test]
    fn test_verbosity_quiet() {
        let cli = Cli {
            config: None,
            verbose: 0,
            quiet: true,
            command: Command::Status(StatusCommand { json: false }),
        };
        assert_eq!(cli.verbosity(), crate::logging::Verbosity::Quiet);
    }

    #[test]
    fn test_verbosity_normal() {
        let cli = Cli {
            config: None,
            verbose: 0,
            quiet: false,
            command: Command::Status(StatusCommand { json: false }),
        };
        assert_eq!(cli.verbosity(), crate::logging::Verbosity::Normal);
    }

    #[test]
    fn test_verbosity_verbose() {
        let cli = Cli {
            config: None,
            verbose: 1,
            quiet: false,
            command: Command::Status(StatusCommand { json: false }),
        };
        assert_eq!(cli.verbosity(), crate::logging::Verbosity::Verbose);
    }

    #[test]
    fn test_verbosity_trace() {
        let cli = Cli {
            config: None,
            verbose: 2,
            quiet: false,
            command: Command::Status(StatusCommand { json: false }),
        };
        assert_eq!(cli.verbosity(), crate::logging::Verbosity::Trace);
    }

    #[test]
    fn test_cli_verify() {
        // Verify the CLI structure is valid
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_daemon_start() {
        let args = vec!["fliterec", "daemon", "start"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(
            cli.command,
            Command::Daemon(DaemonCommand::Start { .. })
        ));
    }

    #[test]
    fn test_parse_status() {
        let args = vec!["fliterec", "status"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.command, Command::Status(_)));
    }

    #[test]
    fn test_parse_search() {
        let args = vec!["fliterec", "search", "test query"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.command, Command::Search(_)));
    }

    #[test]
    fn test_parse_with_config() {
        let args = vec!["fliterec", "-c", "/custom/config.toml", "status"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/custom/config.toml")));
    }

    #[test]
    fn test_parse_with_verbose() {
        let args = vec!["fliterec", "-v", "status"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn test_parse_with_quiet() {
        let args = vec!["fliterec", "-q", "status"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.quiet);
    }
}
