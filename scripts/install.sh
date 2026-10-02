#!/usr/bin/env bash
set -euo pipefail

# Linux Universal Clipboard History Manager Installer
# Supports Debian/Ubuntu, Fedora/RHEL, Arch/Manjaro, openSUSE

BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info() { echo -e "${BLUE}${BOLD}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}${BOLD}[SUCCESS]${NC} $*"; }
warn() { echo -e "${YELLOW}${BOLD}[WARN]${NC} $*"; }
error() { echo -e "${RED}${BOLD}[ERROR]${NC} $*" >&2; }

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
DESKTOP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"

echo -e "${BOLD}======================================================${NC}"
echo -e "${BOLD}  Linux Universal Clipboard History Installer (Win+V) ${NC}"
echo -e "${BOLD}======================================================${NC}"

# 1. Detect Package Manager and Dependencies
install_deps() {
    info "Checking required system dependencies..."
    if command -v apt-get >/dev/null 2>&1; then
        info "Debian/Ubuntu detected. Installing dependencies via apt..."
        sudo apt-get update
        sudo apt-get install -y libgtk-4-dev libadwaita-1-dev libx11-dev libxtst-dev wl-clipboard xdotool pkg-config build-essential
    elif command -v dnf >/dev/null 2>&1; then
        info "Fedora/RHEL detected. Installing dependencies via dnf..."
        sudo dnf install -y gtk4-devel libadwaita-devel libX11-devel libXtst-devel wl-clipboard xdotool pkgconf gcc
    elif command -v pacman >/dev/null 2>&1; then
        info "Arch Linux detected. Installing dependencies via pacman..."
        sudo pacman -S --needed --noconfirm gtk4 libadwaita libx11 libxtst wl-clipboard xdotool pkgconf base-devel
    elif command -v zypper >/dev/null 2>&1; then
        info "openSUSE detected. Installing dependencies via zypper..."
        sudo zypper install -y gtk4-devel libadwaita-devel libX11-devel libXtst-devel wl-clipboard xdotool pkg-config
    else
        warn "Could not detect package manager. Please ensure GTK4, Libadwaita, and X11 development headers are installed."
    fi
}

if [[ "${1:-}" == "--deps" ]]; then
    install_deps
fi

# 2. Build Binaries
info "Building release binaries with Cargo..."
cd "${PROJECT_ROOT}"

# Check if GTK4 dev headers exist to enable full GTK GUI
if pkg-config --exists gtk4 libadwaita-1 2>/dev/null; then
    info "Found GTK4 and Libadwaita dev headers. Building with native GUI support..."
    cargo build --release --workspace --features clipboard-history-gui/gtk
else
    warn "GTK4 or Libadwaita dev headers not found. Building daemon and CLI in release mode..."
    cargo build --release --workspace
fi

# 3. Create Target Directories
mkdir -p "${BIN_DIR}" "${DESKTOP_DIR}" "${ICON_DIR}" "${SYSTEMD_USER_DIR}"

# 4. Install Binaries
info "Installing binaries to ${BIN_DIR}..."
cp -f "${PROJECT_ROOT}/target/release/clipboard-history" "${BIN_DIR}/"
cp -f "${PROJECT_ROOT}/target/release/clipboard-history-daemon" "${BIN_DIR}/"
cp -f "${PROJECT_ROOT}/target/release/clipboard-history-gui" "${BIN_DIR}/"
chmod +x "${BIN_DIR}/clipboard-history"*

# Ensure BIN_DIR is in PATH
if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
    warn "${BIN_DIR} is not in your PATH. Add 'export PATH=\"\$HOME/.local/bin:\$PATH\"' to your ~/.bashrc or ~/.zshrc."
fi

# 5. Install Desktop Entry and Icons
info "Installing desktop entry and icons..."
cp -f "${PROJECT_ROOT}/packaging/desktop/clipboard-history.desktop" "${DESKTOP_DIR}/"
cp -f "${PROJECT_ROOT}/packaging/desktop/icons/clipboard-history.svg" "${ICON_DIR}/"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${DESKTOP_DIR}" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

# 6. Install and Start Systemd User Service
info "Configuring systemd user service..."
cp -f "${PROJECT_ROOT}/packaging/systemd/clipboard-history.service" "${SYSTEMD_USER_DIR}/"

if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload
    systemctl --user enable clipboard-history.service
    systemctl --user restart clipboard-history.service
    success "Systemd user service enabled and started."
fi

# 7. Auto-configure Global Hotkey (Super+V) for GNOME
if command -v gsettings >/dev/null 2>&1 && [[ "${XDG_CURRENT_DESKTOP:-}" == *"GNOME"* ]]; then
    info "Configuring Super+V shortcut in GNOME Settings..."
    SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
    CUSTOM_KEY_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/"
    
    # Add to custom keybindings list if not present
    CURRENT_LIST=$(gsettings get "${SCHEMA}" custom-keybindings 2>/dev/null || echo "[]")
    if [[ "${CURRENT_LIST}" != *"${CUSTOM_KEY_PATH}"* ]]; then
        if [[ "${CURRENT_LIST}" == "@as []" ]] || [[ "${CURRENT_LIST}" == "[]" ]]; then
            NEW_LIST="['${CUSTOM_KEY_PATH}']"
        else
            NEW_LIST=$(echo "${CURRENT_LIST}" | sed "s/]$/, '${CUSTOM_KEY_PATH}']/")
        fi
        gsettings set "${SCHEMA}" custom-keybindings "${NEW_LIST}" || true
    fi

    # Set shortcut attributes
    CUSTOM_SCHEMA="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${CUSTOM_KEY_PATH}"
    gsettings set "${CUSTOM_SCHEMA}" name "Clipboard History" || true
    gsettings set "${CUSTOM_SCHEMA}" command "clipboard-history toggle" || true
    gsettings set "${CUSTOM_SCHEMA}" binding "<Super>v" || true
    success "GNOME Super+V shortcut configured automatically!"
fi

echo -e "\n${GREEN}${BOLD}Installation Complete!${NC}"
echo -e "You can now open Clipboard History by pressing ${BOLD}Super+V${NC} or running ${BOLD}clipboard-history toggle${NC}."
echo -e "Check daemon status with: ${BOLD}clipboard-history status${NC}"
