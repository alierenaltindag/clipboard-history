<p align="center">
  <img src="packaging/desktop/icons/clipboard-history.svg" alt="Clipboard History Logo" width="128" height="128" />
</p>

<h1 align="center">Linux Universal Clipboard History Manager</h1>
<p align="center"><b>The modern, high-performance Windows Win+V alternative for Wayland and X11</b></p>

<p align="center">
  <a href="https://github.com/alierenaltindag/clipboard-history/actions"><img src="https://github.com/alierenaltindag/clipboard-history/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg" alt="License" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.80%2B-orange.svg" alt="Rust" /></a>
  <a href="#compatibility"><img src="https://img.shields.io/badge/Platform-Wayland%20%7C%20X11-success.svg" alt="Platform" /></a>
  <a href="#performance"><img src="https://img.shields.io/badge/Idle%20Memory-%3C15%20MB%20RSS-brightgreen.svg" alt="Memory" /></a>
</p>

> A blazing-fast, modern, privacy-focused Universal Clipboard History utility for Linux desktop environments (**GNOME, KDE Plasma, Cosmic, Hyprland, Sway, Cinnamon, XFCE**). Built from the ground up in **Rust** with native **Wayland & X11** display server support, embedded **SQLite WAL**, and content-addressable **SHA-256 disk blob deduplication**.

---

## 🌟 Key Highlights

- ⚡ **Windows Win+V Ergonomics**: Instant ephemeral popup window with warm-standby activation, cursor-anchored positioning, and modern acrylic card styling.
- 🛡️ **URL De-Tracker & Privacy Cleaner**: Strips telemetry and tracking parameters (`utm_*`, `fbclid`, `gclid`, `si`, `igshid`, `msclkid`, `mc_eid`, Amazon `/ref=`, etc.) with one-click transform in GUI, CLI command `clean-url`, or automatic real-time de-tracking.
- 🚫 **Per-Application Blacklist & Whitelist Manager**: Granular application rules (`blacklist` or `whitelist`) to ignore confidential windows (e.g. terminals, Slack, proprietary software) or restrict capture to specific apps only.
- 🧩 **Multi-Item Batch Operations & "Concatenate Paste"**: Select multiple entries (`Ctrl+M` or Selection Mode) and join them with custom delimiters (newlines, paragraphs, commas, bullet lists, numbered lists) to paste all at once. Batch delete, pin, or enqueue.
- 🔀 **Built-in Clipboard Diff Viewer**: Compare any two text/code snippets with side-by-side/unified syntax-highlighted diffs, additions/deletions statistics, swap controls, and one-click diff copying.
- 🔁 **Sequential Paste Queue (FIFO)**: Queue multiple clipboard items and pop them one by one into any document using the HUD status badge or `clipboard-history queue pop`.
- 📝 **Permanent Snippets & Canned Responses**: Dedicated Snippets tab and modal creation dialog. Dynamic template expansion for `{date}`, `{time}`, `{datetime}`, `{year}`, `{month}`, `{day}`, `{uuid}`, and active `{clipboard}`.
- 🔍 **On-Demand Image OCR**: Extract text directly from copied screenshots and image entries with one click or via `clipboard-history ocr <id>` (graceful `tesseract` integration).
- 🎨 **Color Format Converter & Swatches**: Real-time detection of Hex and RGB/RGBA colors with inline preview swatches and one-click conversions to Hex, RGB, HSL, CSS variables (`--color`), GLSL `vec4`, and SwiftUI `Color`.
- 🔎 **Advanced Search Syntax**: Instant full-text search paired with power directives: `type:code|image|text`, `app:<name>`, `is:pinned`, and `is:snippet`.
- 🌐 **Zero-Trust P2P LAN Encrypted Sync**: Synchronize clipboard events across local devices over TCP with pure-Rust AES-256-GCM encryption derived from a 6-digit pairing PIN and CSPRNG nonces.
- 🐧 **Universal Wayland & X11 Support**: Direct integration via Wayland protocols (`wlr-data-control`, `wl-paste --watch`) and native X11 (`XFixes`, `XTest`, `xdotool`).
- 🗄️ **Zero-Corruption Storage Engine**: Embedded SQLite in WAL mode (`PRAGMA synchronous = NORMAL; PRAGMA mmap_size = 268MB;`) paired with content-addressable SHA-256 blob storage for media.
- 🖼️ **Rich Data Types & Instant Thumbnails**: Plain text, HTML/rich text, file lists (`text/uri-list`), and images (PNG, JPEG, WebP) with downscaled 128×128 thumbnails for lag-free 60–120 FPS scrolling.
- 🛡️ **Enterprise Privacy & Security**:
  - Auto-ignores password manager selections (`x-kde-passwordManagerHint`, `KeepassXC`, `1Password`, `Bitwarden`).
  - Configurable regex filters to mask or block private keys (`-----BEGIN PRIVATE KEY-----`), credit cards, and API tokens.
  - Strict POSIX file permissions (`chmod 0600` on database and media blobs, `0700` on directories).
  - Quick pause / incognito toggle.
- 🎯 **Adaptive Paste Injector Cascade**:
  - Simulates `Ctrl+V` automatically when an item is selected.
  - Automatically cascades across `X11-XTest` ➔ `xdotool` ➔ `wtype` ➔ `ydotool` ➔ seamless copy-only fallback.
- 🧹 **Anti-Bloat Guarantees**:
  - Idle background service footprint **< 15 MB RSS**.
  - Configurable retention policy (TTL expiration and capacity limits) with pinned items protected.
  - Scheduled orphan blob cleanup and SQLite page vacuuming.

---

## 🏗️ Architecture

The project is organized in a clean **hexagonal architecture** within a modular Cargo workspace:

```mermaid
flowchart TD
    subgraph Display Environment
        X11[X11 Display Server]
        Wayland[Wayland Compositors\nGNOME, KDE, Hyprland, Sway]
        App[Target Application Window]
    end

    subgraph Daemon [clipboard-history-daemon]
        Watcher[Display Server Watcher Strategy\nXFixes / wlr-data-control / wl-paste]
        Privacy[Security Filter & Masking\nPassword Manager Guard / Regex Engine]
        Deduplicator[Deduplication & Timestamp Bumping]
        Repo[(SQLite WAL Repository)]
        BlobStore[(SHA-256 Blob Store + Thumbnails)]
        EvictionWorker[Eviction & Vacuum Worker]
        IpcServer[Unix Domain Socket Server]
    end

    subgraph UI & Controls [clipboard-history-gui / CLI]
        Cli[clipboard-history CLI]
        Gui[Libadwaita Win+V Popup Window\nFuzzy Matcher + Filter Tabs]
        Injector[Adaptive Paste Injector Cascade\nXTest / xdotool / wtype / ydotool]
    end

    X11 --> Watcher
    Wayland --> Watcher
    Watcher --> Privacy
    Privacy --> Deduplicator
    Deduplicator --> Repo
    Deduplicator --> BlobStore
    EvictionWorker --> Repo
    EvictionWorker --> BlobStore
    IpcServer <--> Gui
    IpcServer <--> Cli
    Gui -->|Select Item| Injector
    Injector -->|Synthesize Keystroke| App
```

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Super + V` | Open / Toggle Clipboard History popup |
| `↑` / `↓` | Navigate history list |
| `Enter` / Click | Select entry, hide popup, and paste into active window |
| `Shift + Enter` | Paste as plain unformatted text (strips HTML/rich styles) |
| `Ctrl + T` | Open Quick Transforms modal (case converts, JSON, Base64, QR, OCR, colors) |
| `Ctrl + M` | Toggle Multi-Selection Mode for batch operations and diffing |
| `Q` | Enqueue selected item into Sequential Paste Queue (FIFO) |
| `1` – `9` | Quick paste items 1 through 9 |
| `Delete` / `Backspace` | Delete selected entry |
| `P` | Pin / unpin selected entry (prevents auto-eviction) |
| `Esc` | Dismiss / hide popup window |

---

## 🚀 Installation & Quick Start

### Universal Automated Installer
Clone the repository and run the idempotent installer:

```bash
git clone https://github.com/alierenaltindag/clipboard-history.git
cd clipboard-history
./scripts/install.sh
```

To automatically install build dependencies via your distro package manager (`apt`, `dnf`, `pacman`, `zypper`):
```bash
./scripts/install.sh --deps
```

### Standalone Packages
Pre-built packages are generated for all major Linux distributions:
- **Debian / Ubuntu (`.deb`)**:
  ```bash
  ./packaging/deb/build-deb.sh
  sudo dpkg -i target/clipboard-history_*.deb
  ```
- **Fedora / RHEL (`.rpm`)**:
  ```bash
  rpmbuild -ba packaging/rpm/clipboard-history.spec
  ```
- **Arch Linux (`PKGBUILD`)**:
  ```bash
  cd packaging/arch && makepkg -si
  ```
- **AppImage (Portable)**:
  ```bash
  ./packaging/appimage/build-appimage.sh
  ./target/ClipboardHistory-x86_64.AppImage
  ```

---

## 🔧 Desktop Environment Hotkey Setup (Super+V)

### GNOME
The installer automatically registers the `Super+V` shortcut via `gsettings`. To verify or set manually:
```bash
gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "['/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/']"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/ name "Clipboard History"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/ command "clipboard-history toggle"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/ binding "<Super>v"
```

### KDE Plasma
1. Open **System Settings** ➔ **Shortcuts** ➔ **Custom Shortcuts** (or **Command Shortcuts**).
2. Add a new shortcut named `Clipboard History`.
3. Set Trigger to `Meta+V` (Super+V) and Action to `clipboard-history toggle`.

### Hyprland
Add the following line to `~/.config/hypr/hyprland.conf`:
```ini
bind = $mainMod, V, exec, clipboard-history toggle
```

### Sway
Add the following line to `~/.config/sway/config`:
```ini
bindsym $mod+v exec clipboard-history toggle
```

---

## 💻 CLI Commands

The `clipboard-history` command-line utility provides instant control:

```bash
# Toggle popup window
clipboard-history toggle

# List recent entries
clipboard-history list --limit 20
clipboard-history list --type image
clipboard-history list --pinned

# Search clipboard entries with advanced syntax
clipboard-history search "type:code app:github is:pinned token"

# Permanent Snippets & Canned Responses
clipboard-history snippets list
clipboard-history snippets add "Email Sig" "Best regards,\nAli Eren Altındağ" --category "Signatures"
clipboard-history snippets add "Bug Report" "### Context\n- Date: {date}\n- Session ID: {uuid}\n- Log: {clipboard}"
clipboard-history snippets delete <SNIPPET_ID>

# Sequential Paste Queue (FIFO)
clipboard-history queue status
clipboard-history queue add <ENTRY_ID_1> <ENTRY_ID_2> <ENTRY_ID_3>
clipboard-history queue pop      # Pops next item and outputs text
clipboard-history queue clear

# Concatenate & Join Multiple Entries
clipboard-history join <ID_1> <ID_2> <ID_3>                    # Join with newlines
clipboard-history join --delimiter ", " <ID_1> <ID_2>          # Join with commas
clipboard-history join --numbered <ID_1> <ID_2>                # Join as numbered list
clipboard-history join --bullets <ID_1> <ID_2>                 # Join as bulleted list

# Built-in Clipboard Diff Viewer (Colorized terminal diff)
clipboard-history diff <ENTRY_ID_A> <ENTRY_ID_B>
clipboard-history diff <ENTRY_ID_A> <ENTRY_ID_B> --no-color

# Batch Pin and Delete
clipboard-history batch-pin <ID_1> <ID_2> <ID_3>
clipboard-history batch-pin --unpin <ID_1> <ID_2>
clipboard-history batch-delete <ID_1> <ID_2> <ID_3>

# On-Demand Image OCR
clipboard-history ocr <IMAGE_ENTRY_ID_OR_HASH>

# URL De-Tracker & Privacy Cleaner
clipboard-history clean-url "https://example.com?utm_source=twitter&fbclid=123"
clipboard-history clean-url <ENTRY_ID>

# Pin / unpin an entry by ID
clipboard-history pin <ENTRY_ID>
clipboard-history unpin <ENTRY_ID>

# Clear history (preserves pinned items by default)
clipboard-history clear
# Delete everything including pinned items:
clipboard-history clear --all

# Pause / Resume clipboard tracking
clipboard-history pause
clipboard-history resume

# Inspect daemon health & metrics
clipboard-history status

# View and update configuration (takes effect immediately)
clipboard-history config show
clipboard-history config set-clean-urls true        # Auto-strip tracking queries from copied links
clipboard-history config set-app-filter-mode blacklist  # "blacklist" or "whitelist"
clipboard-history config add-app-filter "slack"     # Ignore copies from Slack
clipboard-history config remove-app-filter "slack"
clipboard-history config set-passwords false        # Allow or block password manager copies
clipboard-history config set-incognito false        # Allow or block incognito/private window copies
clipboard-history config set-auto-paste false       # Toggle direct keystroke paste
clipboard-history config set-sync true              # Enable P2P LAN clipboard synchronization
clipboard-history config set-pin "654321"           # Update 6-digit zero-trust pairing PIN
```

---

## ⚙️ Configuration Reference

Settings can be changed visually via the **Preferences** button in the popup header, via the CLI `clipboard-history config`, or by editing `~/.config/clipboard-history/config.toml`:

```toml
[general]
max_entries = 500                # Evicts oldest unpinned items when reached
retention_days = 30              # Evicts unpinned items older than 30 days
max_blob_size_mb = 50            # Rejects payloads larger than 50MB
poll_interval_ms = 300           # Debounce interval
rate_limit_per_second = 10       # Circuit breaker threshold

[security]
ignore_password_managers = true  # Rejects KeePassXC, 1Password, Bitwarden copies (Default: true)
ignore_incognito_windows = true  # Rejects copies made in private/incognito windows (Default: true)
auto_clean_tracking_urls = false # Automatically strip tracking query parameters (utm_*, fbclid, etc.)
app_filter_mode = "blacklist"    # "blacklist" (block listed) or "whitelist" (only allow listed)
app_filter_list = []             # List of filtered application names or window classes
secret_policy = "reject"         # "reject", "mask", or "allow"
encryption_enabled = false       # Pure-Rust AES-256-GCM envelope encryption at rest
custom_secret_patterns = []      # Additional custom regex filters
ignored_window_classes = ["keepassxc", "1password", "bitwarden"]
incognito_window_patterns = [
    "*incognito*",
    "*private browsing*",
    "*tor browser*",
    "*keepass*",
    "*1password*",
    "*bitwarden*"
]

[ui]
theme = "system"                 # "system", "dark", or "light"
window_width = 440
window_height = 580
show_preview_thumbnails = true
tray_icon_enabled = true         # Freedesktop StatusNotifierItem tray icon

[paste]
auto_paste = true                # Directly synthesizes Ctrl+V after selection
paste_delay_ms = 120             # Milliseconds before key injection
preferred_injector = "xtest"     # "xtest", "xdotool", "wtype", "ydotool", or "copy-only"

[sync]
enabled = false                  # Toggle P2P LAN clipboard synchronization
device_name = "linux-desktop"    # Identifying name for this node
listen_port = 54123              # Local TCP listening port
pairing_pin = "123456"           # 6-digit zero-trust pairing key (AES-256-GCM derived)
peer_addresses = []              # Known peer endpoints, e.g. ["192.168.1.150:54123"]

[hotkey]
shortcut = "Super+V"
```

---

## 🛡️ Uninstallation

To remove the binaries, desktop entries, and systemd service:
```bash
./scripts/uninstall.sh
```

To also delete all stored clipboard history and settings:
```bash
./scripts/uninstall.sh --purge
```

---

## 📄 License

This project is dual-licensed under either the [MIT License](LICENSE-MIT) or the [Apache License 2.0](LICENSE-APACHE).
