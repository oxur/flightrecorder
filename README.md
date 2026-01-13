# flightrecorder

[![][build-badge]][build]
[![][crate-badge]][crate]
[![][tag-badge]][tag]
[![][docs-badge]][docs]

[![][logo]][logo-large]

*A system-level service that preserves  ephemeral text input, so that work doesn't disappear after critical failures*

Like the infamous "black box", `flightrecorder` runs quietly in the background, capturing select text input across applications. When disaster strikes—app crashes, network failures, accidental refreshes, or just plain bugs—your work is safe and recoverable.

## The Problem

Web and desktop applications are shockingly bad at preserving your work:

- **App crashes** wipe out that detailed prompt you spent 10 minutes crafting
- **Network errors** swallow form submissions into the void
- **Accidental refreshes** obliterate unsaved text
- **Session timeouts** discard everything without warning
- **Buggy apps** throw errors and auto-refresh, taking your input with them

You shouldn't need to break your workflow, disrupting your chain of thought, just to defensively copy everything to a text editor because applications aren't doing their jobs. That's adding a broken process to a broken tool. A transparent solution is needed.

But *why now*? This has been a problem for decades, why is this experience now so painful as to force the creation of this tool-based solution?

Simply put, it's indirectly due to all the AI work happening on a daily basis. The time that we take to write instructions, prompts, etc., is the distillation of years of experience in problem-solving, given to AIs in a form that will allow them to do vast amounts of work in very short amounts of time. As such, the growing value per word has highlighted new costs for the loss per word. If an hour's worth of writing results in the creation of something of market value in the 10k or 100k currency, the loss of that hour is measurably (eventually) greater now than it used to be. This is a tool to safeguard that value.

## The Solution

`flightrecorder` is your safety net:

- 🔇 **Silent**: Runs as a background daemon, zero interaction required
- 🔒 **Private**: All data stays local, no network access, fully auditable
- 🎯 **Selective**: Captures text fields and clipboard, not raw keystrokes
- 🛡️ **Privacy-aware**: Filters sensitive patterns, ignores password fields
- 🔍 **Searchable**: Find what you lost with powerful search
- 🧹 **Self-maintaining**: Automatic pruning keeps storage bounded
- 🐧🍎 **Cross-platform**: Linux (X11 + Wayland) and macOS

## Installation

```bash
# From source
git clone https://github.com/oxur/flightrecorder.git
cd flightrecorder
cargo build --release
cargo install --path .

# Start the daemon
fliterec daemon start
```

## Usage

```bash
# Check status
fliterec status

# Search your history
fliterec search "that prompt I wrote"

# Recover recent input
fliterec recover --last 10

# Recover from a specific app
fliterec recover --app "Claude" --last 5

# Recover from a time range
fliterec recover --since "1 hour ago"

# Interactive recovery (TUI)
fliterec recover --interactive
```

We also provide the means for users to easily update configuration without having to create a copy of the file(s) in question, etc. Additional commands are used to find the config file on the file system or to display the contents of the file:

```bash
fliterec config set <key> <value>
fliterec config unset <key>
fliterec config path
fliterec config show
```

## How It Works

`flightrecorder` uses two complementary capture strategies:

### 1. Clipboard Monitoring

Every clipboard operation is captured with:

- Timestamp
- Source application (when detectable)
- Content hash (for deduplication)

This catches explicit copies and many form submissions that apps place on the clipboard.

### 2. Accessibility-Based Text Field Snapshots

Using platform accessibility APIs, `flightrecorder` periodically snapshots text from:

- Active text input fields
- Text areas and editors
- Form fields

This provides comprehensive coverage even when you forget to copy.

### Privacy & Security

- **No network access**: The daemon has no ability to transmit data
- **No raw keylogging**: We capture text field *contents*, not individual keystrokes
- **Sensitive data filtering**: Configurable patterns for passwords, API keys, credit cards
- **Password field detection**: Automatically skips password input fields
- **Local storage only**: Everything stays in `~/.local/share/flightrecorder/`
- **Fully open source**: Audit every line of code

## Configuration

```toml
# ~/.config/flightrecorder/config.toml

[capture]
# Snapshot interval for text fields (seconds)
snapshot_interval = 5

# Minimum text length to capture
min_length = 10

# Applications to exclude
exclude_apps = ["1Password", "Bitwarden", "KeePassXC"]

[privacy]
# Patterns to filter (regex)
filter_patterns = [
    "(?i)password",
    "(?i)api[_-]?key",
    "(?i)secret",
    "\\b\\d{4}[- ]?\\d{4}[- ]?\\d{4}[- ]?\\d{4}\\b",  # Credit cards
]

[storage]
# Where to store captured data
data_dir = "~/.local/share/flightrecorder"

# Maximum storage size (MB)
max_size_mb = 500

# Retention period (days)
retention_days = 30
```

## Architecture

The project is organized as a Cargo workspace with platform-specific crates:

```
flightrecorder/
├── Cargo.toml                    # Workspace configuration
├── crates/
│   ├── flightrecorder/           # Main binary crate (CLI: `fliterec`)
│   │   └── src/
│   │       ├── main.rs           # CLI entry point
│   │       ├── cli/              # Command implementations
│   │       ├── daemon/           # Daemon orchestration
│   │       ├── storage/          # Database and pruning
│   │       └── privacy/          # Sensitive data filtering
│   ├── flightrecorder-mac/       # macOS platform library
│   │   └── src/
│   │       ├── clipboard.rs      # Pasteboard monitoring
│   │       └── accessibility.rs  # Accessibility API integration
│   ├── flightrecorder-linux/     # Linux platform library
│   │   └── src/
│   │       ├── clipboard.rs      # X11/Wayland clipboard
│   │       └── accessibility.rs  # AT-SPI integration
│   └── design/                   # Documentation crate (oxur-odm)
│       ├── docs/                 # Managed design documents (ODDs)
│       └── dev/                  # Developer documentation
├── docs/                         # User-facing documentation
│   ├── usage.md                  # CLI and daemon usage
│   ├── privacy.md                # Privacy deep-dive
│   └── platform-support.md       # Platform-specific notes
├── assets/
│   ├── ai/                       # AI assistant resources
│   └── images/                   # Logos and graphics
└── config/
    └── default.toml              # Default configuration
```

Platform-specific code is conditionally compiled via `cfg(target_os = ...)` dependencies in the main binary crate.

## Platform Support

| Platform | Clipboard | Text Field Capture | Status |
|----------|-----------|-------------------|--------|
| macOS | ✅ | ✅ (Accessibility API) | Primary target |
| Linux (X11) | ✅ | ✅ (AT-SPI) | Primary target |
| Linux (Wayland) | ✅ | ⚠️ (Limited by protocol) | Best effort |
| Windows | ❌ | ❌ | Not planned, but contributors welcome |

## Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- daemon start
```

## Comparison to Alternatives

| Feature | flightrecorder | Keyloggers | Clipboard Managers |
|---------|---------------|------------|-------------------|
| Open source | ✅ | Usually ❌ | Sometimes |
| Cross-platform | ✅ | Varies | Varies |
| Privacy-focused | ✅ | ❌ | Partial |
| Captures text fields | ✅ | Via keystrokes | ❌ |
| Captures clipboard | ✅ | Sometimes | ✅ |
| Filters out sensitive data | ✅ | ❌ | ❌ |
| Searchable history | ✅ | Sometimes | Sometimes |
| No raw keylogging | ✅ | ❌ | ✅ |

## Contributing

Contributions welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Areas where help is especially appreciated:

- Windows support
- Wayland text field capture improvements
- Additional platform support
- Privacy pattern suggestions
- UI/UX for the recovery interface

### Setup

To use some of the admin `Makefile` targets, this repo expects that you have the following remotes set up:

```
$ git remote -v
codeberg        ssh://git@codeberg.org/oxur/flightrecorder.git (fetch)
codeberg        ssh://git@codeberg.org/oxur/flightrecorder.git (push)
github  git@github.com:oxur/flightrecorder.git (fetch)
github  git@github.com:oxur/flightrecorder.git (push)
```

- `make push` pushes changes to both code hosting services

### AI

If you are using AI, this repo provides a CLAUDE.md file which expects you to have the oxur/ai-rust guidelines set up in the project. If you have already cloned that repo, you can create a sym link here:

```shell
cd ./assets/ai
ln -s <PATH-TO-AI-RUST-CLONE> ./ai-rust
```

If you don't have ai-rust installed, you can use the following `make` target to do so:

```shell
make ai-rust
```

or run it manually:

```shell
git clone git@github.com:oxur/ai-rust.git ./assets/ai/ai-rust
```

Once you're ready to give an AI agent some instructions, the following tends to work pretty well for Rust coding sessions:

```text
We're going to be working on X in this session.

However, before I go into details with you on that, I need you to do some context preparation:
- please read CLAUDE.me for general knowledge of the project and the basic resources available to you
- that document will have a link to a SKILL.md file; please read!
- you will also be pointed to a collection of Rust best practices, guides, pitfalls, and antipatterns -- please examine for general use and identify the guides that will be most useful for our task at hand
- in particular, we will be exploring X, so look for guides that will best assist in following correct patterns for that topic
```

Or, once you have already started:

```text
We're going to be switching gears to work on Y right now. I need you to first brush up on CLAUDE.md and the useful info in SKILL.md.

In particular, I want you to reexamine the Rust development guides available to you that would be most helpful in correctly planning and writing code for Y.
```

## License

MIT License - See [LICENSE](LICENSE) for details.

---

*Stop losing your work. Start recording what happens in-flight.*

[//]: ---Named-Links---

[logo]: assets/images/logo/v1-x250.png
[logo-large]: assets/images/logo/v1.png
[build]: https://github.com/oxur/flightrecorder/actions/workflows/ci.yml
[build-badge]: https://github.com/oxur/flightrecorder/actions/workflows/ci.yml/badge.svg
[crate]: https://crates.io/crates/flightrecorder
[crate-badge]: https://img.shields.io/crates/v/flightrecorder.svg
[docs]: https://docs.rs/flightrecorder/
[docs-badge]: https://img.shields.io/badge/rust-documentation-blue.svg
[tag-badge]: https://img.shields.io/github/tag/oxur/flightrecorder.svg
[tag]: https://github.com/oxur/flightrecorder/tags
