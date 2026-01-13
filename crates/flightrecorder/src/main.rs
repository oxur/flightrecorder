//! `FlightRecorder` - Cross-platform system recorder
//!
//! This binary provides a unified interface for platform-specific recording functionality.

#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

use clap::{Parser, Subcommand};

// Platform-specific imports using conditional compilation (PS-01)
#[cfg(target_os = "linux")]
use flightrecorder_linux as platform;

#[cfg(target_os = "macos")]
use flightrecorder_mac as platform;

#[derive(Parser, Debug)]
#[command(name = "frec")]
#[command(author, version, about = "FlightRecorder - Your safety net for text input", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Display a detailed description of what frec does
    Desc,

    /// Show the current status (placeholder)
    Status,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Desc) => {
            print_description();
            Ok(())
        }
        Some(Commands::Status) => {
            println!("FlightRecorder - Platform: {}", platform::platform_name());
            platform::init()?;
            println!("Status: Running");
            Ok(())
        }
        None => {
            // Default behavior: show basic info
            println!("FlightRecorder - Platform: {}", platform::platform_name());
            platform::init()?;
            println!("Initialization successful!");
            println!("\nRun 'frec desc' to learn what this tool does.");
            println!("Run 'frec --help' for available commands.");
            Ok(())
        }
    }
}

fn print_description() {
    println!("\n{}", "=".repeat(80));
    println!("FlightRecorder (frec) - What is it?");
    println!("{}\n", "=".repeat(80));

    println!("FlightRecorder is a LOCAL, PRIVACY-FOCUSED safety net for your text input.");
    println!();
    println!("THE PROBLEM:");
    println!("  • Apps crash and lose your carefully written text");
    println!("  • Network errors swallow form submissions");
    println!("  • Accidental refreshes wipe out unsaved work");
    println!("  • You lose hours of work in an instant");
    println!();
    println!("THE SOLUTION:");
    println!("  FlightRecorder silently captures your text input so you can recover it");
    println!("  when disaster strikes - like an airplane's black box.");
    println!();
    println!("WHAT IT DOES:");
    println!("  ✓ Monitors clipboard operations (when you copy text)");
    println!("  ✓ Takes snapshots of active text fields (what you're typing)");
    println!("  ✓ Stores everything LOCALLY in ~/.local/share/flightrecorder/");
    println!("  ✓ Lets you search and recover lost text");
    println!();
    println!("PRIVACY & SECURITY:");
    println!("  ✓ NO network access - cannot transmit data anywhere");
    println!("  ✓ NO keylogging - captures text field contents, not keystrokes");
    println!("  ✓ Filters sensitive data (passwords, API keys, credit cards)");
    println!("  ✓ Skips password fields automatically");
    println!("  ✓ ALL data stays on YOUR machine");
    println!("  ✓ Fully open source - audit every line of code");
    println!();
    println!("PLATFORM:");
    println!("  Running on: {}", platform::platform_name());
    println!();
    println!("FOR MORE INFO:");
    println!("  • Full documentation: README.md in the project root");
    println!("  • Source code: https://github.com/yourusername/flightrecorder");
    println!("  • Privacy details: docs/PRIVACY.md");
    println!();
    println!("{}", "=".repeat(80));
    println!("Your work matters. Never lose it again.");
    println!("{}\n", "=".repeat(80));
}
