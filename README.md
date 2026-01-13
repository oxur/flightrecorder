# flightrecorder

[![][build-badge]][build]
[![][crate-badge]][crate]
[![][tag-badge]][tag]
[![][docs-badge]][docs]

[![][logo]][logo-large]

*A system-level service that preserves  ephemeral text input, so that work doesn't disappear after critical failures*

Like an airplane's black box, `flightrecorder` runs quietly in the background, capturing your text input across applications. When disaster strikes—app crashes, network failures, accidental refreshes, or just plain bugs—your work is safe and recoverable.

## The Problem

Modern applications are shockingly bad at preserving your work:

- **App crashes** wipe out that detailed prompt you spent 10 minutes crafting
- **Network errors** swallow form submissions into the void
- **Accidental refreshes** obliterate unsaved text
- **Session timeouts** discard everything without warning
- **Buggy apps** throw errors and auto-refresh, taking your input with them

The psychological toll is real: the anxiety of potential data loss poisons the entire experience of using otherwise-good tools. You shouldn't need to defensively copy everything to a text editor "just in case."

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
git clone https://github.com/YOUR_USERNAME/flightrecorder.git
cd flightrecorder
cargo build --release
cargo install --path .

# Start the daemon
flightrecorder daemon start
```

## Usage

```bash
# Check status
flightrecorder status

# Search your history
flightrecorder search "that prompt I wrote"

# Recover recent input
flightrecorder recover --last 10

# Recover from a specific app
flightrecorder recover --app "Claude" --last 5

# Recover from a time range
flightrecorder recover --since "1 hour ago"

# Interactive recovery (TUI)
flightrecorder recover --interactive
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

```
flightrecorder/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── daemon/
│   │   ├── mod.rs           # Daemon orchestration
│   │   ├── clipboard.rs     # Clipboard monitoring
│   │   └── accessibility.rs # Text field snapshots
│   ├── platform/
│   │   ├── mod.rs           # Platform abstraction
│   │   ├── macos/           # macOS implementations
│   │   └── linux/           # Linux (X11 + Wayland)
│   ├── storage/
│   │   ├── mod.rs           # Storage abstraction
│   │   ├── database.rs      # SQLite storage
│   │   └── pruning.rs       # Automatic cleanup
│   ├── privacy/
│   │   ├── mod.rs           # Privacy filtering
│   │   └── patterns.rs      # Sensitive data detection
│   └── cli/
│       ├── mod.rs           # CLI commands
│       ├── search.rs        # Search functionality
│       └── recover.rs       # Recovery interface
├── config/
│   └── default.toml         # Default configuration
└── docs/
    ├── ARCHITECTURE.md      # Detailed design docs
    ├── PRIVACY.md           # Privacy deep-dive
    └── PLATFORM_SUPPORT.md  # Platform-specific notes
```

## Platform Support

| Platform | Clipboard | Text Field Capture | Status |
|----------|-----------|-------------------|--------|
| macOS | ✅ | ✅ (Accessibility API) | Primary target |
| Linux (X11) | ✅ | ✅ (AT-SPI) | Primary target |
| Linux (Wayland) | ✅ | ⚠️ (Limited by protocol) | Best effort |
| Windows | ❌ | ❌ | Not planned |

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
| Filters sensitive data | ✅ | ❌ | ❌ |
| Searchable history | ✅ | Sometimes | Sometimes |
| No raw keylogging | ✅ | ❌ | ✅ |

## Why "flightrecorder"?

Like an airplane's flight data recorder (black box):

- Runs silently in the background
- You forget it's there until you need it
- Invaluable for disaster recovery
- Captures just enough to reconstruct what happened

## Contributing

Contributions welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Areas where help is especially appreciated:

- Wayland text field capture improvements
- Additional platform support
- Privacy pattern suggestions
- UI/UX for the recovery interface

### Setup

This repo requires that you have the following remotes set up:

```
$ git remote -v
codeberg        ssh://git@codeberg.org/oxur/flightrecorder.git (fetch)
codeberg        ssh://git@codeberg.org/oxur/flightrecorder.git (push)
github  git@github.com:oxur/flightrecorder.git (fetch)
github  git@github.com:oxur/flightrecorder.git (push)
```

- `make push` pushes changes to both code hosting services

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
