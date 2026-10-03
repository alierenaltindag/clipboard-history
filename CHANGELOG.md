# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - 2026-10-03

### Added
- **Modern Libadwaita GUI & High-Performance Rendering**:
  - Full Libadwaita styling and theme alignment (`adw::Application`, `adw::WindowTitle`, `adw::StatusPage`, `adw::ToastOverlay`).
  - Zero-allocation Cairo `DrawingArea` rendering for instant color preview swatches, eliminating dynamic CSS provider allocations.
  - Smooth acrylic CSS transitions (`180ms cubic-bezier(0.2, 0, 0, 1)`) and modern rounded surface design.
  - Search performance optimization capping rendered entries to the top 50 matches for 60 FPS fluid interaction.
  - Reorganized Settings dialog with 3 dedicated `adw::PreferencesPage` tabs (General, Privacy & Security, Network Sync).
  - Rich item rows with keyboard `<kbd>` badges, content type tags, relative human-readable timestamps, and image previews.
- **Universal Remote Installer**:
  - Direct one-line remote execution via `curl -fsSL https://raw.githubusercontent.com/alierenaltindag/clipboard-history/main/scripts/install.sh | bash` without needing to clone the repository manually.
  - Cross-distribution dependency detection and package management support for Debian/Ubuntu (`apt`), Fedora/RHEL (`dnf`), Arch Linux (`pacman`), and openSUSE (`zypper`).
  - Automatic Rust compiler bootstrapping via `rustup` when missing.
  - Automated GNOME `Super+V` shortcut configuration.

### Changed
- **GUI-First Configuration**:
  - Removed CLI `config` subcommands in favor of centralized GUI preferences dialog.
  - Cleaned and streamlined `README.md` to highlight quick installation, uninstallation, general features, and complete keyboard shortcuts.

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
