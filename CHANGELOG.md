# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.11] - 2026-10-25

### Fixed
- **Wayland & CachyOS/KDE Plasma Window Lifecycle and Dock Stability**:
  - **Eliminated Focus-Loss Auto-Dismiss Loop**:
    - The auto-dismiss handler (`connect_is_active_notify`) now tracks whether the window was actually focused by the user/compositor (`has_been_focused`), preventing newly mapped Wayland windows from prematurely closing before KWin or the user assigns active focus.
    - Added an initial presentation grace period (1200ms) ensuring the window stays visible during Wayland compositor mapping animations and keyboard navigation.
  - **Removed Disruptive `xdotool` Invocations on Wayland**:
    - Purged invalid `xdotool windowactivate` and `xdotool windowmove` fallbacks on native Wayland GTK4 surfaces (`xid_opt.is_none()`). These calls previously targeted Xwayland and caused KWin to flicker or de-focus the native Wayland surface.
  - **Unified Dock Application ID & Eliminated Duplicate Taskbar Icons**:
    - Added `StartupWMClass=com.antigravity.ClipboardHistory` to `clipboard-history.desktop` and installed desktop and icon symlinks under `com.antigravity.ClipboardHistory`. This enables KDE Plasma, GNOME Dash, and other docks to associate the running window with the pinned launcher, resolving the duplicate dock icon issue.
    - Updated Hyprland window rules and move dispatch to match both `class:clipboard-history-gui` and `class:com.antigravity.ClipboardHistory`.
  - **Multi-Path Binary Discovery & Update Priority**:
    - Updated `find_binary_named()` in `crates/core/src/service.rs` to prioritize `~/.local/bin` and `~/.cargo/bin` over `/usr/bin`, ensuring user in-place updates are discovered and launched immediately without being shadowed by older system packages.
  - **Background Launch Support**:
    - Added `--hidden` and `--background` CLI arguments to `clipboard-history-gui`, allowing silent startup without triggering premature window presentation during session restore.

## [1.2.10] - 2026-10-25

### Fixed
- **Wayland & CachyOS Clipboard Capture Reliability**:
  - Resolved `wl-paste` SIGPIPE termination loop: child processes in `--watch` mode now properly drain standard input (`sh -c 'cat > /dev/null; echo change'`), preventing Linux kernel pipe break signals and the resulting 10-second watcher backoff loop.
  - Implemented resilient fallback text retrieval: if `wl-paste --list-types` or a specific MIME type fails or returns empty, the watcher automatically queries generic `wl-paste --no-newline` rather than dropping the clipboard event.
  - Added live `UnixStream` socket connectivity checking in `ensure_display_environment()`: scans active `wayland-0` through `wayland-9` sockets and automatically selects the running compositor instance, resolving SDDM multi-socket desync on CachyOS/KDE.
  - Added graceful fallback polling loop: automatically transitions to 400ms periodic clipboard polling if the active compositor does not support the `wlr-data-control` protocol (e.g. GNOME Mutter on Wayland).
  - Extended `wl-paste` timeout threshold to 2.5s with descriptive failure logging and added `wl-paste` dependency reporting to the GUI Service Diagnostics window.

## [1.2.9] - 2026-10-24

### Fixed
- **Network Sync Unpair Asymmetry & State Desynchronization**:
  - Implemented `clean_ip_address` across all network sync components, stripping ports and normalizing IPv6-mapped IPv4 representations (e.g., `::ffff:192.168.1.50` -> `192.168.1.50`) so dual-stack socket connections match stored trust configurations regardless of Linux distribution differences (Ubuntu vs. CachyOS).
  - Added alias-aware unpairing in daemon server and peer tracker: unpairing cleanses both IP addresses and device hostnames simultaneously across `trusted_peers` and `peer_addresses`.
  - Added multi-candidate unpair broadcast targeting both active network ports (54123, 54124) and all known peer IP addresses when unpairing.
  - Implemented automatic self-healing on background peer discovery: if a peer reports that the local device is not paired (`AuthResponse { success: false, message: "Peer not paired..." }`), the local device automatically purges the remote peer from its `trusted_peers` and `peer_addresses`.
  - Updated GUI Settings Dialog unpair action to dispatch unpair requests for both IP address and device display name.

- **Pairing Notification Spam & Triple UI Redundancy**:
  - Eliminated duplicate notifications: when a desktop notification banner with Accept/Decline action buttons is already displayed (`desktop_notification_active = true`), the intrusive modal `PairingDialog` popup is suppressed.
  - Implemented D-Bus notification lifecycle closing (`close_notification` via `org.freedesktop.Notifications.CloseNotification`): accepting, declining, or timing out a pairing request immediately dismisses the active desktop notification, preventing lingering zombie notifications.
  - Added request deduplication in daemon server: incoming pairing requests from an already pending device or IP address cancel prior timers and dismiss their notifications before registering the new request.
  - Fixed fallback `notify-send` invocation to prevent dual notification banners on systems using shell fallbacks.

## [1.2.8] - 2026-10-24

### Fixed
- **In-App Updater UX & Application Restart Handover**:
  - Implemented seamless GUI restart handover (`restart_gui_application`) with D-Bus lifecycle management, preventing stale processes from remaining in RAM after binary installation.
  - Added dynamic real-time daemon version synchronization in the Settings Dialog and Main Window banner: dynamically detects when a newer daemon is online on disk and transforms the update button into an actionable "Restart App" button.
  - Implemented async IPC polling in GUI and CLI after update application, waiting for the daemon to finish restarting before confirming success.
  - Replaced hardcoded `~/.local/bin` destination with intelligent multi-target binary directory resolution (`determine_install_dir`), supporting custom writable installation locations.
  - Hardened daemon service restart (`restart_daemon_service`) with `daemon-reload`, `reset-failed`, and standalone process fallback.

## [1.2.7] - 2026-10-24

### Performance
- **End-to-End Build Pipeline Acceleration**:
  - Replaced heavyweight `bundled-full` feature on `rusqlite` with lean `["bundled", "chrono", "blob"]`, stripping 25+ unused SQLite C extensions and slashing SQLite C compilation time.
  - Tuned release profile with `opt-level = 2` and local-crate LTO (`lto = false`) with `codegen-units = 16`, eliminating the 70+ second binary linking bottleneck on `clipboard-history-daemon` and `clipboard-history-gui`.
  - Added native `sccache` compiler caching detection and auto-configuration in `scripts/install.sh` for near-instant 3-5 second subsequent rebuilds.
  - Added automated `sccache` dependency installation across Arch/CachyOS, Debian/Ubuntu, Fedora, and openSUSE.

## [1.2.6] - 2026-10-24

### Performance
- **Parallel Compilation & Build Acceleration**:
  - Replaced single-threaded fat LTO (`codegen-units = 1`) with multi-threaded ThinLTO (`lto = "thin"`, `codegen-units = 16`), drastically cutting release build times across all processor cores.
  - Added native multi-threaded `mold` linker auto-detection and execution in `scripts/install.sh`.
  - Added automatic installation of `mold` and `clang` across Arch/CachyOS, Debian/Ubuntu, Fedora, and openSUSE package managers in `scripts/install.sh`.

### Fixed
- **Systemd Service Self-Healing & Rate-Limit Recovery**:
  - Added automatic `systemctl --user reset-failed` execution prior to service restart in `smart_start_daemon` and `scripts/install.sh`, automatically clearing `start-limit-hit` state after rapid restart loops.
- **Code Formatting**:
  - Formatted codebase conforming to Rust standard style guidelines (`cargo fmt`).

## [1.2.5] - 2026-10-19

### Added
- **Intelligent Daemon Service Manager & Self-Healing**:
  - Implemented `smart_start_daemon()` in core with multi-path binary discovery (`std::env::current_exe()`, PATH, `/usr/bin`, `/usr/local/bin`, `~/.local/bin`, `~/.cargo/bin`).
  - Added automatic user systemd unit generation and installation into `~/.config/systemd/user/clipboard-history.service` if not found in distribution directories.
  - Added automatic stale/dead socket detection and cleanup to prevent startup blocking.
  - Added progressive retry loop (up to 2.5s) to guarantee connection during first-time cold-start database and cryptographic key initialization.
- **Service Diagnostics & Troubleshooting Dialog**:
  - Added Libadwaita diagnostic window showing executable path, systemd service state, display server environment (`WAYLAND_DISPLAY`, `DISPLAY`, `XDG_SESSION_TYPE`), and recent journalctl service logs.
  - Included a one-click "Copy Diagnostics" button and "Auto-Repair & Start Service" button.
- **Interactive Offline Empty State**:
  - Replaced static error message with an actionable offline card with "Start Service" (with active progress indicator) and "Troubleshoot & Logs".

### Fixed
- **Systemd Compatibility on CachyOS / Arch / Hyprland / Sway**:
  - Removed `Requisite=graphical-session.target` which caused service startup failures on window managers and compositors where `graphical-session.target` is not bound.
  - Updated installation target to `WantedBy=default.target` to ensure reliable autostart across all desktop environments and window managers.

## [1.2.4] - 2026-10-19

### Fixed
- **Complete English Localization for Update System**:
  - Fully translated all user-facing update notifications, action buttons ("Update Now", "Later"), window banner ("A newer version is available!", "Update Now", "Dismiss"), and status labels to English.
  - Replaced all legacy Turkish strings in desktop notifications and GUI components.
- **Robust Offline Fallback for Update Checks**:
  - Implemented direct GitHub API check in GUI Preferences and CLI when daemon is offline or connection fails.
  - Resolved "Unexpected response from daemon" by improving error handling and adding automatic fallback.
- **SQLite Database Schema Migration Resilience**:
  - Fixed `is_pinned` snippet index initialization order to prevent `no such column: is_pinned` on databases created by earlier versions.

## [1.2.3] - 2026-10-19

### Fixed
- **Network Sync Pinned Items Distribution**:
  - Attached full entry payload, blob data, and thumbnail payloads to `SyncMessage::PinSync` so peers that do not yet possess the entry locally receive, store, and pin it immediately.
  - Implemented unpin distribution over network sync, reliably clearing pin state across all connected peers.
  - Added support for snippet pin toggling across network sync via `snippet_` hash prefix handling in `set_pinned_by_hash`.
  - Added periodic live refresh in GUI when window is open to immediately reflect changes synced from peers.

### Added
- **Software Updates & Auto-Update Notifications**:
  - Integrated GitHub release checking and one-click in-app update banner and settings dialog.
  - Added FreeDesktop update notification dispatch with actionable update prompt.
  - Added CLI `clipboard-history update [check|install]` command.

## [1.2.2] - 2026-10-19

### Fixed
- **Pin State Persistence & Unpin Reversion Prevention**:
  - Removed aggressive force-repin override logic (`if entry.is_pinned && existing_pinned == 0`) from `insert_or_update` and `batch_insert_or_update` in SQLite storage repository, preventing background history streaming from resetting unpinned items to pinned.
  - Added `set_pinned_by_hash` to `SqliteRepository` and the `Storage` trait to reliably toggle pin states across network sync without requiring UUIDs.
  - Implemented `is_pinned` column persistence in the `snippets` table with automated schema migration (`ALTER TABLE snippets ADD COLUMN is_pinned ...`).
  - Added fallback in `set_pinned`, `batch_set_pinned`, `delete`, and `batch_delete` to ensure canned snippets can be pinned, unpinned, and deleted directly from the main view.
  - Fixed GUI snippet mapping so snippets reflect their actual `is_pinned` state instead of being hardcoded to `true`.
  - Fixed GUI pin toggling and deletion on search-filtered views to update both `entries` and `filtered_entries`.

### Added
- **Real-Time Cross-Device Pin State Synchronization**:
  - Introduced `SyncMessage::PinSync { origin_device, content_hash, is_pinned }` in the network sync protocol.
  - Broadcast `PinSync` over TCP to all paired peers upon pinning or unpinning entries via daemon IPC (`PinEntry`, `UnpinEntry`, `BatchPin`).
  - Handled incoming `PinSync` messages in daemon TCP listener and sync service to keep peer SQLite databases immediately and bidirectionally consistent.

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
