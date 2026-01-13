---
number: 1
title: "flightrecorder Project Plan"
author: "content and"
component: All
tags: [change-me]
created: 2026-01-13
updated: 2026-01-13
state: Active
supersedes: null
superseded-by: null
version: 1.0
---

# flightrecorder Project Plan

## Executive Summary

**flightrecorder** is a system-level service that preserves ephemeral text input, protecting users from data loss due to app crashes, network failures, and accidental refreshes. It runs as a background daemon, capturing clipboard changes and text field contents via accessibility APIs, storing them locally in SQLite for later search and recovery.

This plan outlines a phased implementation approach, starting with macOS support and a complete capture stack (clipboard + accessibility), then extending to Linux with appropriate fallbacks for Wayland's accessibility limitations.

---

## Discovery Summary

### Key Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Capture strategy** | Layered: clipboard → accessibility → keylogging | Maximum coverage with graceful degradation |
| **Platform priority** | macOS first, Linux second | Focus on one platform while designing abstractions for both |
| **Async runtime** | Tokio throughout | AT-SPI is async, unified runtime simplifies architecture |
| **IPC mechanism** | Unix domain socket | Standard, simple, cross-platform for POSIX systems |
| **Daemon management** | launchd/systemd service files | Modern best practice - don't daemonize in code |
| **Crate structure** | Platform crates (mac/linux) + extract as needed | Flexible organization, split when cross-platform patterns emerge |
| **Privacy filtering** | Module in main crate | Start simple, extract if complexity grows |
| **MVP scope** | Daemon + clipboard + accessibility (macOS) | Complete capture stack from day one |

### Recommended Dependencies

| Component | Crate | Version | Notes |
|-----------|-------|---------|-------|
| CLI | `clap` | 4.x | Already in workspace |
| Async runtime | `tokio` | 1.x | Full features |
| Clipboard | `clipboard-rs` | latest | Has monitoring/change detection |
| macOS Accessibility | `macos-accessibility-client` | latest | Direct Apple API bindings |
| Linux AT-SPI | `atspi` | latest | Pure Rust, async (Odilia project) |
| Keylogging | `rdev` | latest | Cross-platform base |
| Linux evdev | `evdev` | latest | Wayland fallback |
| SQLite | `rusqlite` | latest | Bundles SQLite, sync is fine |
| TUI | `ratatui` | latest | Interactive recovery |
| Config | `figment` | 0.10.x | Hierarchical, integrates with clap |
| Serialization | `serde` | 1.x | With derive |
| Logging | `tracing` | latest | Async-friendly logging |
| Error handling | `thiserror` + `anyhow` | latest | Library + application errors |

### Platform Considerations

**macOS:**

- Requires Accessibility API permission (System Preferences > Privacy > Accessibility)
- Uses launchd for service management
- Clipboard via NSPasteboard, text fields via AXUIElement

**Linux X11:**

- AT-SPI works well for accessibility
- Clipboard via X11 selections
- No special permissions typically needed

**Linux Wayland:**

- Accessibility is broken except on GNOME (no protocol exists)
- Clipboard works via wl-clipboard protocols
- Fallback to evdev keylogging requires `input` group membership
- Accept reduced functionality on non-GNOME Wayland

**Privacy Messaging Note:**
The current README says "NO keylogging" but our plan includes evdev as a fallback for Wayland. This needs careful messaging:

- **Preferred mode:** Text field content capture (not keystrokes) - this is the default
- **Fallback mode (opt-in):** Keystroke capture on Wayland when accessibility is unavailable
- The fallback should be clearly documented and require explicit user opt-in
- Consider a `--enable-keystroke-fallback` flag or config option

---

## High-Level Vision

```
┌─────────────────────────────────────────────────────────────────┐
│                        fliterec CLI                             │
│  (status, search, recover, daemon start/stop, config)           │
└─────────────────────────────────────────────────────────────────┘
                              │ Unix Socket IPC
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     fliterec Daemon                             │
│  ┌─────────────┐   ┌───────────────┐   ┌───────────────┐        │
│  │  Clipboard  │   │ Accessibility │   │  Keylog       │        │
│  │  Monitor    │   │  Monitor      │   │  (fallback)   │        │
│  └──────┬──────┘   └───────┬───────┘   └───────┬───────┘        │
│         │                  │                   │                │
│         ▼                  ▼                   ▼               │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                  Privacy Filter                           │  │
│  │  (pattern matching, password field detection)             │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                  │
│                              ▼                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                  Storage (SQLite)                         │  │
│  │  (captures, metadata, deduplication, pruning)             │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Phase Overview

| Phase | Focus | Outcome |
|-------|-------|---------|
| **1** | Foundation | Core types, storage, config, CLI scaffolding |
| **2** | macOS Capture | Clipboard + accessibility monitoring |
| **3** | Daemon & IPC | Background service, socket communication |
| **4** | CLI & Recovery | Full commands, search, TUI recovery |
| **5** | Linux Support | AT-SPI, evdev fallback, systemd |
| **6** | Polish | Pruning, docs, testing, release prep |

---

## Phase 1: Foundation

**Goal:** Establish core infrastructure that all other phases build upon.

### Stage 1.1: Core Types & Traits

**Work:**

- Define `Capture` struct (timestamp, source app, content, content_hash, capture_type)
- Define `CaptureType` enum (Clipboard, TextField, Keystroke)
- Define `CaptureSource` trait for platform monitors to implement
- Define `Error` types with `thiserror`
- Set up `tracing` for structured logging

**Files:**

- `crates/flightrecorder/src/capture.rs`
- `crates/flightrecorder/src/error.rs`
- `crates/flightrecorder/src/lib.rs`

**Success Criteria:**

- [ ] Core types compile and have appropriate derives (Debug, Clone, Serialize, Deserialize)
- [ ] Error types cover all anticipated failure modes
- [ ] `cargo test` passes with unit tests for types

### Stage 1.2: Storage Layer

**Work:**

- Create SQLite schema (captures table, metadata table)
- Implement `Storage` struct with rusqlite
- Methods: `insert_capture`, `search`, `get_recent`, `get_by_app`, `prune_old`
- Content hashing for deduplication
- Database migrations pattern

**Files:**

- `crates/flightrecorder/src/storage/mod.rs`
- `crates/flightrecorder/src/storage/schema.rs`
- `crates/flightrecorder/src/storage/migrations.rs`

**Success Criteria:**

- [ ] Can create database at `~/.local/share/flightrecorder/captures.db`
- [ ] CRUD operations work correctly
- [ ] Deduplication prevents duplicate captures
- [ ] Search by content and by app works
- [ ] Integration tests pass

### Stage 1.3: Configuration

**Work:**

- Define `Config` struct with serde + figment
- Support TOML config file at `~/.config/flightrecorder/config.toml`
- Environment variable overrides
- Default values for all settings
- Validation of config values

**Files:**

- `crates/flightrecorder/src/config.rs`
- `config/default.toml` (embedded default)

**Success Criteria:**

- [ ] Config loads from file, env vars, and defaults in correct precedence
- [ ] Invalid config produces helpful error messages
- [ ] `fliterec config show` displays current configuration

### Stage 1.4: CLI Scaffolding

**Work:**

- Set up clap with derive for command structure
- Subcommands: `daemon`, `status`, `search`, `recover`, `config`
- Global flags: `--config`, `--verbose`, `--quiet`
- Help text and version info

**Files:**

- `crates/flightrecorder/src/main.rs`
- `crates/flightrecorder/src/cli/mod.rs`
- `crates/flightrecorder/src/cli/commands.rs`

**Success Criteria:**

- [ ] `fliterec --help` shows all commands
- [ ] `fliterec --version` shows version
- [ ] Command parsing works correctly
- [ ] Subcommand stubs print "not yet implemented"

---

## Phase 2: macOS Capture

**Goal:** Implement complete text capture on macOS.

### Stage 2.1: Clipboard Monitoring

**Work:**

- Integrate `clipboard-rs` for macOS
- Implement clipboard change detection loop
- Extract source application when possible
- Handle all clipboard content types (text focus)
- Async task for continuous monitoring

**Files:**

- `crates/flightrecorder-mac/src/clipboard.rs`
- `crates/flightrecorder-mac/src/lib.rs`

**Success Criteria:**

- [ ] Detects clipboard changes within 1 second
- [ ] Captures text content correctly
- [ ] Identifies source application
- [ ] Runs as async task without blocking
- [ ] Graceful handling of non-text clipboard content

### Stage 2.2: Accessibility Text Field Capture

**Work:**

- Integrate `macos-accessibility-client`
- Request/check accessibility permissions
- Find focused text field in active application
- Periodic snapshot of text field contents (configurable interval)
- Handle permission denied gracefully

**Files:**

- `crates/flightrecorder-mac/src/accessibility.rs`
- `crates/flightrecorder-mac/src/permissions.rs`

**Success Criteria:**

- [ ] Detects when accessibility permission is missing
- [ ] Provides clear instructions to grant permission
- [ ] Captures text from focused text fields
- [ ] Respects snapshot interval configuration
- [ ] Skips password fields automatically

### Stage 2.3: Privacy Filtering

**Work:**

- Implement configurable regex-based filtering
- Built-in patterns: passwords, API keys, credit cards, SSNs
- Password field detection via accessibility attributes
- Application exclusion list (1Password, Bitwarden, etc.)
- Content sanitization before storage

**Files:**

- `crates/flightrecorder/src/privacy/mod.rs`
- `crates/flightrecorder/src/privacy/patterns.rs`
- `crates/flightrecorder/src/privacy/filter.rs`

**Success Criteria:**

- [ ] Built-in patterns match expected sensitive data
- [ ] Custom patterns can be added via config
- [ ] Excluded apps are never captured
- [ ] Password fields are detected and skipped
- [ ] Filtered content never reaches storage

### Stage 2.4: Platform Abstraction

**Work:**

- Define `CaptureMonitor` trait in main crate
- Implement trait for macOS monitors
- Create platform-agnostic capture orchestrator
- Handle monitor lifecycle (start, stop, restart)

**Files:**

- `crates/flightrecorder/src/monitor.rs`
- `crates/flightrecorder-mac/src/monitor.rs`

**Success Criteria:**

- [ ] Platform code is cleanly separated
- [ ] Main crate has no platform-specific code
- [ ] Monitors can be started/stopped independently
- [ ] Same interface will work for Linux implementation

---

## Phase 3: Daemon & IPC

**Goal:** Run capture as a background service with CLI control.

### Stage 3.1: Unix Socket IPC

**Work:**

- Define IPC message protocol (JSON over Unix socket)
- Messages: Status, Search, Recover, Shutdown, Reload
- Implement server (daemon) side
- Implement client (CLI) side
- Handle connection errors gracefully

**Files:**

- `crates/flightrecorder/src/ipc/mod.rs`
- `crates/flightrecorder/src/ipc/protocol.rs`
- `crates/flightrecorder/src/ipc/server.rs`
- `crates/flightrecorder/src/ipc/client.rs`

**Success Criteria:**

- [ ] Socket created at `~/.local/share/flightrecorder/fliterec.sock`
- [ ] CLI can send commands to daemon
- [ ] Daemon responds with appropriate data
- [ ] Connection timeout handling works
- [ ] Stale socket file is cleaned up on startup

### Stage 3.2: Daemon Process

**Work:**

- Tokio-based async main loop
- Start all capture monitors
- Listen on Unix socket for commands
- Graceful shutdown on SIGTERM/SIGINT
- PID file management
- Status reporting (uptime, capture counts)

**Files:**

- `crates/flightrecorder/src/daemon/mod.rs`
- `crates/flightrecorder/src/daemon/server.rs`
- `crates/flightrecorder/src/daemon/signals.rs`

**Success Criteria:**

- [ ] `fliterec daemon start` launches daemon
- [ ] `fliterec daemon stop` stops daemon gracefully
- [ ] `fliterec status` shows daemon status and stats
- [ ] Daemon survives terminal close (detached)
- [ ] Only one daemon instance can run at a time

### Stage 3.3: launchd Integration

**Work:**

- Create launchd plist template
- `fliterec daemon install` command to install service
- `fliterec daemon uninstall` to remove service
- Auto-start on login configuration
- Log file routing

**Files:**

- `crates/flightrecorder/src/daemon/launchd.rs`
- `assets/macos/com.oxur.fliterec.plist`

**Success Criteria:**

- [ ] Service installs to `~/Library/LaunchAgents/`
- [ ] Service starts on login if configured
- [ ] `launchctl` commands work correctly
- [ ] Logs go to appropriate location
- [ ] Uninstall cleanly removes service

---

## Phase 4: CLI & Recovery

**Goal:** Full CLI functionality including interactive recovery.

### Stage 4.1: Search Command

**Work:**

- Full-text search in captured content
- Filter by app, date range, capture type
- Output formats: plain, JSON, table
- Pagination for large result sets
- Highlight matching text

**Files:**

- `crates/flightrecorder/src/cli/search.rs`

**Success Criteria:**

- [ ] `fliterec search "query"` finds matching captures
- [ ] `--app`, `--since`, `--until` filters work
- [ ] `--format json` outputs valid JSON
- [ ] Large result sets are paginated
- [ ] Exit code indicates match/no-match

### Stage 4.2: Recover Command

**Work:**

- Recover recent captures: `--last N`
- Recover by app: `--app "AppName"`
- Recover by time: `--since "1 hour ago"`
- Output to stdout or clipboard
- Multiple output formats

**Files:**

- `crates/flightrecorder/src/cli/recover.rs`

**Success Criteria:**

- [ ] `fliterec recover --last 10` shows recent captures
- [ ] `--to-clipboard` copies to clipboard
- [ ] Time parsing works with human-friendly formats
- [ ] Clear output formatting
- [ ] Helpful when no matches found

### Stage 4.3: Interactive TUI Recovery

**Work:**

- Integrate `ratatui` for terminal UI
- List view of captures with preview
- Fuzzy search/filter
- Select and copy to clipboard
- Keyboard navigation

**Files:**

- `crates/flightrecorder/src/cli/tui/mod.rs`
- `crates/flightrecorder/src/cli/tui/app.rs`
- `crates/flightrecorder/src/cli/tui/ui.rs`

**Success Criteria:**

- [ ] `fliterec recover --interactive` launches TUI
- [ ] Can navigate captures with arrow keys
- [ ] Can search/filter captures
- [ ] Can copy selected capture to clipboard
- [ ] Clean exit with q or Esc

---

## Phase 5: Linux Support

**Goal:** Extend all functionality to Linux.

### Stage 5.1: Linux Clipboard

**Work:**

- `clipboard-rs` already supports Linux
- Test X11 clipboard monitoring
- Test Wayland clipboard (wl-clipboard backend)
- Handle both clipboard and primary selection

**Files:**

- `crates/flightrecorder-linux/src/clipboard.rs`

**Success Criteria:**

- [ ] Clipboard monitoring works on X11
- [ ] Clipboard monitoring works on Wayland
- [ ] Primary selection is optionally captured
- [ ] Same interface as macOS implementation

### Stage 5.2: AT-SPI Accessibility

**Work:**

- Integrate `atspi` crate
- Connect to AT-SPI D-Bus interface
- Monitor focused text elements
- Periodic text field snapshots
- Handle D-Bus connection errors

**Files:**

- `crates/flightrecorder-linux/src/accessibility.rs`
- `crates/flightrecorder-linux/src/atspi.rs`

**Success Criteria:**

- [ ] Works on X11 desktop environments
- [ ] Works on GNOME Wayland
- [ ] Graceful degradation on other Wayland (warning + disable)
- [ ] Same interface as macOS implementation

### Stage 5.3: evdev Keylogging Fallback

**Work:**

- Integrate `evdev` crate for /dev/input access
- Detect when accessibility is unavailable
- Convert key events to text (keyboard layout aware)
- Buffer keystrokes into "words" or "lines"
- Require `input` group membership

**Files:**

- `crates/flightrecorder-linux/src/keylog.rs`
- `crates/flightrecorder-linux/src/evdev.rs`

**Success Criteria:**

- [ ] Detects correct keyboard device
- [ ] Converts keycodes to characters
- [ ] Handles modifier keys (shift, caps)
- [ ] Buffers into meaningful chunks
- [ ] Clear error if not in `input` group

### Stage 5.4: systemd Integration

**Work:**

- Create systemd user service file
- `fliterec daemon install` for Linux
- Auto-start configuration
- Journal logging integration

**Files:**

- `crates/flightrecorder/src/daemon/systemd.rs`
- `assets/linux/fliterec.service`

**Success Criteria:**

- [ ] Service installs to `~/.config/systemd/user/`
- [ ] `systemctl --user` commands work
- [ ] Logs visible in `journalctl --user`
- [ ] Auto-start on login works

---

## Phase 6: Polish

**Goal:** Production-ready release.

### Stage 6.1: Storage Pruning

**Work:**

- Automatic pruning based on retention period
- Pruning based on storage size limit
- Pruning runs on schedule (daily)
- Manual prune command

**Files:**

- `crates/flightrecorder/src/storage/pruning.rs`

**Success Criteria:**

- [ ] Old captures auto-deleted based on config
- [ ] Storage stays under size limit
- [ ] `fliterec prune` manually triggers cleanup
- [ ] Pruning doesn't block capture

### Stage 6.2: Documentation

**Work:**

- README updates with final usage
- Man page generation
- User docs in `docs/`
- Architecture docs in `crates/design/dev/`

**Files:**

- `README.md`
- `docs/usage.md`
- `docs/privacy.md`
- `docs/platform-support.md`

**Success Criteria:**

- [ ] README accurately reflects final functionality
- [ ] Installation instructions are complete
- [ ] Privacy documentation is comprehensive
- [ ] Platform-specific notes are accurate

### Stage 6.3: Testing

**Work:**

- Unit tests for all modules
- Integration tests for storage
- Integration tests for IPC
- Platform-specific tests (macOS, Linux)
- CI configuration

**Files:**

- `tests/integration_*.rs`
- `.github/workflows/ci.yml`

**Success Criteria:**

- [ ] 80%+ code coverage
- [ ] CI passes on macOS and Linux
- [ ] All critical paths have tests
- [ ] No clippy warnings

### Stage 6.4: Release Preparation

**Work:**

- Version bump and changelog
- Cargo.toml metadata complete
- Binary releases (GitHub Actions)
- Homebrew formula (macOS)
- AUR package (Arch Linux)

**Success Criteria:**

- [ ] `cargo publish` ready
- [ ] GitHub releases with binaries
- [ ] Installation via package managers works
- [ ] Release notes are comprehensive

---

## Critical Files Summary

### Main Binary Crate (`crates/flightrecorder/`)

- `src/main.rs` - Entry point
- `src/lib.rs` - Library exports
- `src/capture.rs` - Core types
- `src/config.rs` - Configuration
- `src/error.rs` - Error types
- `src/storage/` - SQLite storage
- `src/privacy/` - Privacy filtering
- `src/daemon/` - Daemon management
- `src/ipc/` - CLI-daemon communication
- `src/cli/` - CLI commands
- `src/monitor.rs` - Capture orchestration

### macOS Platform Crate (`crates/flightrecorder-mac/`)

- `src/lib.rs` - Platform exports
- `src/clipboard.rs` - Clipboard monitoring
- `src/accessibility.rs` - AX text field capture
- `src/permissions.rs` - Permission handling
- `src/monitor.rs` - Platform monitor implementation

### Linux Platform Crate (`crates/flightrecorder-linux/`)

- `src/lib.rs` - Platform exports
- `src/clipboard.rs` - X11/Wayland clipboard
- `src/accessibility.rs` - AT-SPI integration
- `src/keylog.rs` - evdev fallback
- `src/monitor.rs` - Platform monitor implementation

---

## Verification Plan

### After Each Phase

1. **Build check:** `cargo build --all-features`
2. **Lint check:** `cargo clippy --all-features -- -D warnings`
3. **Test check:** `cargo test --all-features`
4. **Format check:** `cargo fmt -- --check`

### End-to-End Testing

1. **macOS (after Phase 4):**
   - Install and start daemon
   - Copy text to clipboard → verify capture
   - Type in text field → verify capture
   - Search for captured text
   - Recover via TUI

2. **Linux (after Phase 5):**
   - Same tests on X11
   - Same tests on GNOME Wayland
   - Verify graceful degradation on other Wayland

---

## Dependencies to Add to Workspace

```toml
[workspace.dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Clipboard
clipboard-rs = "0.2"

# Storage
rusqlite = { version = "0.32", features = ["bundled"] }

# TUI
ratatui = "0.29"
crossterm = "0.28"

# Configuration
figment = { version = "0.10", features = ["toml", "env"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Time
chrono = { version = "0.4", features = ["serde"] }

# Regex for privacy patterns
regex = "1.10"

# Hashing for deduplication
blake3 = "1.5"
```

### macOS-specific

```toml
[target.'cfg(target_os = "macos")'.dependencies]
macos-accessibility-client = "0.0.1"
```

### Linux-specific

```toml
[target.'cfg(target_os = "linux")'.dependencies]
atspi = "0.22"
evdev = "0.12"
zbus = "4.4"
```
