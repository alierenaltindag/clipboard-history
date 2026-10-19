# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.1] - 2026-10-19

### Fixed
- **Network Sync State Consistency & Mutual Unpairing**:
  - Implemented complete unpair cleanup across all peer identifiers (`clean_ip`, `IP:port`, and device name from discovery tracker) in both `trusted_peers` and `peer_addresses`.
  - Added bi-directional `SyncMessage::UnpairResponse` protocol handshake to ensure unpair signals are transmitted, processed, acknowledged, and persisted across peers on ports `54123` and `54124`.
  - Fixed asymmetric connection state between differing distributions (CachyOS/Arch and Ubuntu).
  - Invalidated cached authorization upon unpair: strictly enforce trust verification in `AuthRequest` so unshared or revoked peers cannot authenticate.
  - Automatically reload configuration in GUI Settings dialog (`IpcRequest::GetConfig`) to prevent stale `manual_peers` from retaining unpaired state.

### Added
- **Cross-Distribution Actionable Pairing Notifications & Prompts**:
  - Integrated FreeDesktop Notifications D-Bus specification (`org.freedesktop.Notifications`) using `zbus` with actionable buttons (`Accept` / `Decline`) and asynchronous `ActionInvoked` signal listening.
  - Provided robust fallback across Linux distributions supporting both `-A` (Arch/CachyOS) and `--action` (Ubuntu/Debian) flags.
  - Added native GTK4/Libadwaita modal pairing dialog in the GUI application that presents an interactive prompt whenever a peer sends a pairing request.

## [1.2.0] - 2026-10-16

### Added
- **Universal Full-History GUI Search**:
  - Connected the GUI search input to asynchronous IPC search (`IpcRequest::Search`) querying the entire SQLite FTS5 database via daemon IPC.
  - Monotonic sequence counter (`search_seq`) to prevent asynchronous race conditions and discard stale out-of-order query responses.
  - Instantaneous (<1ms) synchronous restoration of recent chronological cache upon clearing search input.
- **Context-Aware Terminal Paste Injection**:
  - Added intelligent active window classification covering 46+ terminal emulators (`alacritty`, `kitty`, `gnome-terminal`, `konsole`, `wezterm`, `foot`, `tilix`, etc.).
  - Bounded and delimited matching to eliminate false positives on graphical apps (Steam, Postman, JetBrains IDEs).
  - Automatically dispatches `Ctrl+Shift+V` for terminals and `Ctrl+V` for standard desktop applications across all injectors.

### Fixed
- **Database Concurrency & Transient Lock Resilience**:
  - Configured SQLite connection initialization with 5000ms `busy_timeout` via connection setup and live `PRAGMA busy_timeout = 5000`.
  - Upgraded write transactions to `TransactionBehavior::Immediate`, preventing lock upgrade deadlocks and `SQLITE_BUSY` errors during concurrent access from daemon, GUI, and CLI.
- **Wayland Process Safety & Focus Delay Calibration**:
  - Enforced `cmd.kill_on_drop(true)` on `wl-paste` child processes, preventing zombie process leaks during clipboard execution timeouts.
  - Calibrated auto-paste delay with a minimum 120ms floor on Wayland sessions to ensure compositor window focus settles before keystroke injection.
- **Code Quality & Clippy Cleanliness**:
  - Resolved all workspace compiler and clippy warnings under strict `-D warnings` mode.

## [1.1.1] - 2026-10-16

### Added
- **Multi-Device Network Sync Clear All Propagation**:
  - Added `SyncMessage::ClearHistorySync` protocol message to broadcast history wipeouts across all paired, authenticated devices.
  - Executing "Clear All" (via GUI or CLI `clipboard-history clear`) automatically purges history and cleans up orphaned image blobs across the entire trusted mesh network.
  - Option to preserve or purge pinned items is synchronized seamlessly across peers (`include_pinned`).

## [1.1.0] - 2026-10-16

### Added
- **High-Accuracy OCR Engine & Turkish Language Optimization**:
  - Full support for Turkish characters (`ç, ğ, ı, ö, ş, ü, Ç, Ğ, İ, Ö, Ş, Ü`) and multilingual text recognition.
  - Multi-pass Page Segmentation: Defaults to `--psm 6` (single uniform text block) for clipboard snippets with fallback to `--psm 3` and `--psm 11` for sparse text.
  - Subpixel LCD anti-aliasing neutralization via ITU-R 601 grayscale conversion and dynamic contrast normalization.
  - Adaptive Lanczos3 upscaling to 300 DPI equivalent with 20px solid white border padding.
  - Automatic discovery and prioritization of unquantized float LSTM neural models (`tessdata_best`).
  - Strict exclusion of `osd` from text recognition passes, eliminating LSTM engine crashes.
- **OCR Daemon & GUI Integration**:
  - Encryption-at-rest support for OCR image blobs (`read_decrypted`) and encrypted envelope DEK storage of newly generated text entries.
  - Non-blocking asynchronous loading indicator (`⏳ Extracting text (OCR)...`) in the Quick Transforms popover.
  - Interactive missing dependency dialog with one-click terminal launcher for automated package installation across Debian, Ubuntu, Arch, Fedora, openSUSE.
  - Clipboard echo prevention via `prepare_paste`.
- **Packaging & Installation**:
  - Automated `tessdata_best` high-accuracy model downloads in `install.sh` and terminal installer.
  - CLI `clipboard-history ocr <id|hash>` command for fast terminal-based text extraction.

## [1.0.0] - 2026-10-13

### Added
- **Windows 11 Fluent Ergonomics & Compact Layout**:
  - Compact horizontal window profile and sleek card geometry matching modern desktop clipboard managers.
  - Right-aligned vertical quick actions stack:
    - 3-dot contextual menu popover containing source app and exact copy timestamp.
    - Direct Pin/Unpin action button with dynamic visual feedback.
    - Direct Delete action button for immediate item removal.
  - Smart character-limited single-line previews (`truncate_preview_text`) normalizing whitespace with automatic ellipsis (`...`) for long content.
  - Collapsible Pinned Items accordion section header with interactive chevron toggle and dynamic live counter (`📌 Pinned (N)`).
  - Pinned items persist safely across system reboots, logouts, and daemon restarts in embedded SQLite WAL.
  - Streamlined card presentation: removed redundant type badges for cleaner aesthetics while retaining inline Cairo color swatches and image thumbnails.
  - Modernized Quick Transforms & Actions modal with smooth rounded corners, clean metadata badges, and responsive action layout.
- **Daemon Lifecycle Management & Systemd Resiliency**:
  - Added `clipboard-history daemon` command (`start`, `stop`, `restart`, `status`) to control the daemon directly from the CLI.
  - Automatic daemon startup in GUI with seamless recovery and a dedicated "Start Service" button in the offline state.
  - Hardened systemd user service with `/usr/bin/env` binary resolution and `default.target` session binding for Arch, CachyOS, Fedora, and Ubuntu.
  - Enhanced installer dependency detection to guarantee `wl-clipboard` is installed on Wayland systems.

### Changed
- Refactored row layout and keyboard navigation for fluid vertical scanning and enhanced density.
- Removed footer key indicators in favor of a clean, distraction-free interface.
- Full formatting alignment (`cargo fmt`) and strict compiler/clippy zero-warning policy (`cargo clippy -- -D warnings`).

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
