# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-10-02

### Added
- **Core Engine (`crates/core`)**:
  - Embedded SQLite WAL repository with memory-mapped I/O and synchronous `NORMAL`.
  - Content-addressable SHA-256 disk blob deduplication under `~/.local/share/clipboard-history/blobs/`.
  - Fast downscaling image thumbnail generator (128×128 px).
  - Password manager suppression (`x-kde-passwordManagerHint`, KeePassXC, 1Password, Bitwarden).
  - Configurable regex secret masking for private keys, AWS tokens, GitHub PATs, and credit cards.
  - Bounded thread-safe in-memory LRU cache.
  - Length-delimited Unix domain socket IPC protocol.
- **Daemon (`crates/daemon`)**:
  - Background tracking daemon with < 9 MB RSS memory consumption.
  - Native X11 XFixes clipboard watcher (`x11rb`).
  - Wayland data-control watcher with `wl-paste --watch` fallback.
  - Scheduled TTL and capacity eviction worker with orphaned blob cleanup.
  - Circuit breaker throttling (>10 events/sec) and payload size guard (>50MB).
  - Systemd user service unit with strict sandboxing.
- **Popup GUI (`crates/gui`)**:
  - Libadwaita/GTK4 Win+V style floating popup window.
  - Sub-50ms warm-standby activation.
  - Real-time fuzzy search (`fuzzy-matcher`) with category filter tabs.
  - Keyboard navigation (arrows, Enter, Del, P to pin, Esc, 1-9 quick paste).
  - Adaptive paste injector cascade (`X11-XTest`, `xdotool`, `wtype`, `ydotool`, copy-only).
- **CLI Utility (`crates/cli`)**:
  - Standalone `clipboard-history` management command.
- **Distribution & Packaging**:
  - Idempotent `scripts/install.sh` and `scripts/uninstall.sh`.
  - Debian (`.deb`), RPM (`.spec`), Arch (`PKGBUILD`), and AppImage builders.
  - GitHub Actions CI and automated Release workflows.
