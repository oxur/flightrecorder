//! `fliterec` - CLI for flightrecorder
//!
//! This binary provides the command-line interface for managing the flightrecorder
//! daemon and interacting with captured text.

#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use clap::Parser;

use flightrecorder::cli::{Cli, Command, ConfigCommand, DaemonCommand};
use flightrecorder::{init_logging, Config};

// Platform-specific imports using conditional compilation
#[cfg(target_os = "linux")]
use flightrecorder_linux as platform;

#[cfg(target_os = "macos")]
use flightrecorder_mac as platform;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    init_logging(cli.verbosity());

    // Load configuration
    let config = Config::load_from(cli.config.clone())?;

    // Execute the command
    match cli.command {
        Command::Daemon(daemon_cmd) => handle_daemon(&daemon_cmd),
        Command::Status(status_cmd) => handle_status(&config, status_cmd.json),
        Command::Search(search_cmd) => {
            handle_search(&search_cmd);
            Ok(())
        }
        Command::Recover(recover_cmd) => {
            handle_recover(&recover_cmd);
            Ok(())
        }
        Command::Config(config_cmd) => handle_config(&config, config_cmd),
    }
}

fn handle_daemon(cmd: &DaemonCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        DaemonCommand::Start { foreground } => {
            if *foreground {
                println!("Starting daemon in foreground mode...");
            } else {
                println!("Starting daemon...");
            }
            println!("Platform: {}", platform::platform_name());
            platform::init()?;
            println!("[Not yet implemented]");
        }
        DaemonCommand::Stop { force } => {
            if *force {
                println!("Force stopping daemon...");
            } else {
                println!("Stopping daemon...");
            }
            println!("[Not yet implemented]");
        }
        DaemonCommand::Restart { foreground } => {
            println!(
                "Restarting daemon{}...",
                if *foreground { " in foreground" } else { "" }
            );
            println!("[Not yet implemented]");
        }
        DaemonCommand::Install { start } => {
            println!("Installing daemon as system service...");
            if *start {
                println!("Will start after installation.");
            }
            println!("[Not yet implemented]");
        }
        DaemonCommand::Uninstall { stop } => {
            if *stop {
                println!("Stopping and uninstalling daemon...");
            } else {
                println!("Uninstalling daemon...");
            }
            println!("[Not yet implemented]");
        }
    }
    Ok(())
}

fn handle_status(config: &Config, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        let status = serde_json::json!({
            "daemon_running": false,
            "platform": platform::platform_name(),
            "database_path": config.database_path(),
            "status": "not_implemented"
        });
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("fliterec status");
        println!("---------------");
        println!("Platform:      {}", platform::platform_name());
        println!("Daemon:        Not running");
        println!("Database:      {}", config.database_path().display());
        println!();
        println!("[Full status not yet implemented]");
    }
    Ok(())
}

fn handle_search(cmd: &flightrecorder::cli::SearchCommand) {
    println!("Searching for: \"{}\"", cmd.query);
    if let Some(app) = &cmd.app {
        println!("  Filtered by app: {app}");
    }
    if let Some(capture_type) = &cmd.capture_type {
        println!("  Filtered by type: {capture_type:?}");
    }
    println!("  Limit: {}", cmd.limit);
    println!("  Format: {:?}", cmd.format);
    println!();
    println!("[Search not yet implemented]");
}

fn handle_recover(cmd: &flightrecorder::cli::RecoverCommand) {
    if cmd.interactive {
        println!("Launching interactive recovery TUI...");
        println!("[Interactive mode not yet implemented]");
        return;
    }

    if let Some(n) = cmd.last {
        println!("Recovering last {n} captures...");
    } else if let Some(app) = &cmd.app {
        println!("Recovering captures from app: {app}");
    } else if let Some(since) = &cmd.since {
        println!("Recovering captures since: {since}");
    } else {
        println!("Recovering recent captures...");
    }

    if cmd.to_clipboard {
        println!("  Will copy to clipboard");
    }
    println!("  Format: {:?}", cmd.format);
    println!();
    println!("[Recovery not yet implemented]");
}

fn handle_config(config: &Config, cmd: ConfigCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ConfigCommand::Show { json } => {
            if json {
                println!("{}", serde_json::to_string_pretty(config)?);
            } else {
                println!("Current Configuration");
                println!("=====================");
                println!();
                println!("[Storage]");
                println!("  Database path:      {}", config.database_path().display());
                println!("  Max captures:       {}", config.storage.max_captures);
                println!("  Max age (days):     {}", config.storage.max_age_days);
                println!();
                println!("[Capture]");
                println!("  Clipboard:          {}", config.capture.clipboard_enabled);
                println!(
                    "  Accessibility:      {}",
                    config.capture.accessibility_enabled
                );
                println!(
                    "  Keystroke fallback: {}",
                    config.capture.keystroke_fallback_enabled
                );
                println!();
                println!("[Privacy]");
                println!("  Filters enabled:    {}", config.privacy.filters_enabled);
                println!(
                    "  Skip passwords:     {}",
                    config.privacy.skip_password_fields
                );
                println!(
                    "  Excluded apps:      {}",
                    config.privacy.excluded_apps.len()
                );
            }
        }
        ConfigCommand::Path => {
            println!("{}", Config::default_config_path().display());
        }
        ConfigCommand::Edit => {
            println!("Opening configuration in editor...");
            println!("Config path: {}", Config::default_config_path().display());
            println!("[Editor launch not yet implemented]");
        }
        ConfigCommand::Reset { yes } => {
            if yes {
                println!("Resetting configuration to defaults...");
            } else {
                println!("This will reset all configuration to defaults.");
                println!("Use --yes to confirm.");
            }
            println!("[Reset not yet implemented]");
        }
        ConfigCommand::Validate { file } => {
            let path = file.unwrap_or_else(Config::default_config_path);
            println!("Validating configuration: {}", path.display());
            match Config::load_from(Some(path)) {
                Ok(_) => println!("Configuration is valid."),
                Err(e) => println!("Configuration error: {e}"),
            }
        }
    }
    Ok(())
}
