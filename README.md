<p align="center">
  <img src="packaging/desktop/icons/clipboard-history.svg" alt="Clipboard History Logo" width="128" height="128" />
</p>

<h1 align="center">Linux Universal Clipboard History Manager</h1>
<p align="center"><b>The modern, high-performance Windows Win+V alternative for Wayland and X11</b></p>

<p align="center">
  <a href="https://github.com/alierenaltindag/clipboard-history/actions"><img src="https://github.com/alierenaltindag/clipboard-history/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg" alt="License" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.80%2B-orange.svg" alt="Rust" /></a>
  <img src="https://img.shields.io/badge/Platform-Wayland%20%7C%20X11-success.svg" alt="Platform" />
  <img src="https://img.shields.io/badge/Idle%20Memory-%3C15%20MB%20RSS-brightgreen.svg" alt="Memory" />
</p>

> A blazing-fast, modern, privacy-focused Universal Clipboard History utility for Linux desktop environments (**GNOME, KDE Plasma, Cosmic, Hyprland, Sway, Cinnamon, XFCE**). Built in **Rust** with native **Wayland & X11** display server support, Libadwaita GUI styling, embedded **SQLite WAL**, and content-addressable **SHA-256 disk blob deduplication**.

---

## 🌟 Key Features

- ⚡ **Windows Win+V Ergonomics**: Instant ephemeral popup window with warm-standby activation, smooth 60fps animations, crossfade view switching, and modern Libadwaita acrylic card styling.
- ⚙️ **All Settings Managed via GUI**: Every setting (retention limits, privacy rules, auto-paste, per-app filters, and encrypted sync) is configured directly inside the modern graphical **Preferences** dialog — no manual config editing needed.
- 🛡️ **URL De-Tracker & Privacy Cleaner**: Strips telemetry and tracking parameters (`utm_*`, `fbclid`, `gclid`, `si`, `igshid`, `msclkid`, `mc_eid`, Amazon `/ref=`, etc.) with one click or automatically in real time.
- 🚫 **Per-Application Blacklist & Whitelist Filter**: Granular application rules to ignore sensitive windows (terminals, password managers, private chats) or restrict capture exclusively to allowed apps.
- 🧩 **Multi-Item Batch Operations & "Concatenate Paste"**: Select multiple entries (`Ctrl+M`) and join them with custom delimiters (newlines, paragraphs, commas, bullet lists, numbered lists) to paste all at once. Batch delete, pin, or enqueue.
- 🔀 **Built-in Clipboard Diff Viewer**: Compare any two text or code snippets with side-by-side/unified syntax-highlighted diffs, additions/deletions stats, swap controls, and one-click diff copying.
- 🔁 **Sequential Paste Queue (FIFO)**: Queue multiple clipboard items and pop them one by one into any document using the HUD status badge or `Q` key.
- 📝 **Permanent Snippets & Canned Responses**: Dedicated Snippets tab with dynamic template placeholders (`{date}`, `{time}`, `{datetime}`, `{year}`, `{month}`, `{day}`, `{uuid}`, and `{clipboard}`).
- 🔍 **On-Demand Image OCR**: Extract text directly from copied screenshots and image entries with one click.
- 🎨 **Color Format Converter & Swatches**: Real-time detection of Hex and RGB/RGBA colors with inline Cairo swatches and one-click conversions to Hex, RGB, HSL, CSS variables (`--color`), and GLSL.
- 🖼️ **Image Thumbnails & Rich Previews**: Copied images display scaled thumbnail previews directly in the card. Code entries render inside formatted monospace blocks with character and line count indicators.
- 🌐 **Zero-Trust P2P LAN Encrypted Sync**: Synchronize clipboard items across trusted local devices with pure-Rust AES-256-GCM encryption derived from a 6-digit pairing PIN.
- 🐧 **Universal Wayland & X11 Support**: Direct integration via Wayland protocols (`wlr-data-control`, `wl-paste`) and native X11 (`XFixes`, `XTest`, `xdotool`).
- 🛡️ **Enterprise Privacy Guard**: Auto-ignores password manager selections (`KeePassXC`, `1Password`, `Bitwarden`) and private/incognito browsing windows.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Super + V` | Open / Toggle Clipboard History popup |
| `↑` / `↓` | Navigate history list |
| `Enter` / Click | Select entry, hide popup, and paste into active window |
| `Shift + Enter` | Paste as plain unformatted text (strips rich styles) |
| `Ctrl + T` | Open Quick Transforms modal (case converts, JSON, Base64, QR, OCR, colors) |
| `Ctrl + M` | Toggle Multi-Selection Mode for batch operations and diffing |
| `Q` | Enqueue selected item into Sequential Paste Queue (FIFO) |
| `1` – `9` | Quick paste items 1 through 9 |
| `Delete` / `Backspace` | Delete selected entry |
| `P` | Pin / unpin selected entry (prevents auto-eviction) |
| `Esc` | Dismiss / hide popup window |

---

## 🚀 Installation

### One-Line Global Install (Recommended)

Run this single command in your terminal. It works across **all major Linux distributions** (Ubuntu/Debian, Fedora/RHEL, Arch/Manjaro, openSUSE) without needing to manually clone the repository:

```bash
curl -fsSL https://raw.githubusercontent.com/alierenaltindag/clipboard-history/main/scripts/install.sh | bash
```

*Or via `wget`:*
```bash
wget -qO- https://raw.githubusercontent.com/alierenaltindag/clipboard-history/main/scripts/install.sh | bash
```

The installer automatically:
1. Detects your distribution and verifies required system dependencies.
2. Builds the optimized release binaries.
3. Installs binaries to `~/.local/bin/` with application icons and desktop entries.
4. Enables and starts the background systemd service (`clipboard-history.service`).
5. Configures the global `Super+V` shortcut automatically on GNOME.

---

### Manual Installation (From Source)

If you prefer to clone and build locally:

```bash
git clone https://github.com/alierenaltindag/clipboard-history.git
cd clipboard-history
./scripts/install.sh
```

---

### Standalone Packages

Pre-built packaging scripts are available in `packaging/`:
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

## 🔧 Desktop Environment Shortcut Setup (Super+V)

### GNOME
Configured automatically by the installer. To configure or verify manually:
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
Add to `~/.config/hypr/hyprland.conf`:
```ini
bind = $mainMod, V, exec, clipboard-history toggle
```

### Sway
Add to `~/.config/sway/config`:
```ini
bindsym $mod+v exec clipboard-history toggle
```

---

## 💻 CLI Commands

The `clipboard-history` command-line utility provides scriptable control:

```bash
# Toggle popup window
clipboard-history toggle

# List recent history entries
clipboard-history list --limit 20
clipboard-history list --type image
clipboard-history list --pinned

# Search clipboard entries
clipboard-history search "query text"

# Permanent Snippets
clipboard-history snippets list
clipboard-history snippets add "Email Signature" "Best regards,\nYour Name" --category "General"
clipboard-history snippets delete <SNIPPET_ID>

# Sequential Paste Queue (FIFO)
clipboard-history queue status
clipboard-history queue pop
clipboard-history queue clear

# Concatenate & Join Multiple Entries
clipboard-history join <ID_1> <ID_2> <ID_3>
clipboard-history join --delimiter ", " <ID_1> <ID_2>
clipboard-history join --numbered <ID_1> <ID_2>
clipboard-history join --bullets <ID_1> <ID_2>

# Visual Diff Comparison
clipboard-history diff <ENTRY_ID_A> <ENTRY_ID_B>

# Batch Pin / Delete
clipboard-history batch-pin <ID_1> <ID_2>
clipboard-history batch-delete <ID_1> <ID_2>

# Image OCR
clipboard-history ocr <IMAGE_ENTRY_ID_OR_HASH>

# URL De-Tracker
clipboard-history clean-url "https://example.com?utm_source=tracker&fbclid=123"

# Pin / unpin an entry by ID
clipboard-history pin <ENTRY_ID>
clipboard-history unpin <ENTRY_ID>

# Clear history (preserves pinned items by default; use --all to purge)
clipboard-history clear
clipboard-history clear --all

# Pause / Resume clipboard tracking
clipboard-history pause
clipboard-history resume

# Inspect daemon health & status
clipboard-history status
```

> **Note**: All application settings (retention days, max entries, privacy rules, auto-paste, and network sync) are configured directly in the **GUI Preferences** window (click the gear icon in the titlebar or run `clipboard-history-gui --settings`).

---

## 🗑️ Uninstallation

### One-Line Uninstall
```bash
curl -fsSL https://raw.githubusercontent.com/alierenaltindag/clipboard-history/main/scripts/uninstall.sh | bash
```

To also delete all stored clipboard history and database files:
```bash
curl -fsSL https://raw.githubusercontent.com/alierenaltindag/clipboard-history/main/scripts/uninstall.sh | bash -s -- --purge
```

### Local Script
```bash
./scripts/uninstall.sh
# Or with complete history purge:
./scripts/uninstall.sh --purge
```

---

## 📄 License

This project is dual-licensed under either the [MIT License](LICENSE-MIT) or the [Apache License 2.0](LICENSE-APACHE).
