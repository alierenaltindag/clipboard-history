#!/usr/bin/env bash
set -euo pipefail

# Linux Universal Clipboard History Manager Installer
# One-liner supported: curl -fsSL https://raw.githubusercontent.com/alierenaltindag/clipboard-history/main/scripts/install.sh | bash
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

echo -e "${BOLD}======================================================${NC}"
echo -e "${BOLD}  Linux Universal Clipboard History Installer (Win+V) ${NC}"
echo -e "${BOLD}======================================================${NC}"

# 1. Determine Repository Source (Local Clone or Remote URL)
REPO_URL="https://github.com/alierenaltindag/clipboard-history.git"
TEMP_DIR=""

# Check if script is executed within an existing local repository clone
CURRENT_DIR="$(pwd)"
SCRIPT_DIR=""
if [[ -n "${BASH_SOURCE[0]:-}" ]] && [[ -f "${BASH_SOURCE[0]}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

if [[ -f "${CURRENT_DIR}/Cargo.toml" ]] && grep -q "clipboard-history" "${CURRENT_DIR}/Cargo.toml" 2>/dev/null; then
    PROJECT_ROOT="${CURRENT_DIR}"
    info "Using current repository directory: ${PROJECT_ROOT}"
elif [[ -n "${SCRIPT_DIR}" ]] && [[ -f "${SCRIPT_DIR}/../Cargo.toml" ]]; then
    PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
    info "Using local repository at: ${PROJECT_ROOT}"
else
    info "Running standalone installer. Cloning repository from GitHub..."
    TEMP_DIR="$(mktemp -d -t clipboard-history-install-XXXXXX)"
    trap 'if [[ -n "${TEMP_DIR}" && -d "${TEMP_DIR}" ]]; then rm -rf "${TEMP_DIR}"; fi' EXIT
    if ! command -v git >/dev/null 2>&1; then
        info "Installing git..."
        if command -v apt-get >/dev/null 2>&1; then
            sudo apt-get update && sudo apt-get install -y git
        elif command -v dnf >/dev/null 2>&1; then
            sudo dnf install -y git
        elif command -v pacman >/dev/null 2>&1; then
            sudo pacman -S --needed --noconfirm git
        elif command -v zypper >/dev/null 2>&1; then
            sudo zypper install -y git
        fi
    fi
    git clone --depth 1 "${REPO_URL}" "${TEMP_DIR}"
    PROJECT_ROOT="${TEMP_DIR}"
fi

BIN_DIR="${HOME}/.local/bin"
DESKTOP_DIR="${HOME}/.local/share/applications"
METAINFO_DIR="${HOME}/.local/share/metainfo"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"

# 2. Check and Install System Dependencies
install_dependencies() {
    info "Verifying required desktop and build libraries..."
    if command -v apt-get >/dev/null 2>&1; then
        info "Debian/Ubuntu detected. Installing dependencies via apt..."
        sudo apt-get update
        sudo apt-get install -y libgtk-4-dev libadwaita-1-dev libx11-dev libxtst-dev wl-clipboard xdotool pkg-config build-essential curl git
    elif command -v dnf >/dev/null 2>&1; then
        info "Fedora/RHEL detected. Installing dependencies via dnf..."
        sudo dnf install -y gtk4-devel libadwaita-devel libX11-devel libXtst-devel wl-clipboard xdotool pkgconf gcc curl git
    elif command -v pacman >/dev/null 2>&1; then
        info "Arch Linux detected. Installing dependencies via pacman..."
        sudo pacman -S --needed --noconfirm gtk4 libadwaita libx11 libxtst wl-clipboard xdotool pkgconf base-devel curl git
    elif command -v zypper >/dev/null 2>&1; then
        info "openSUSE detected. Installing dependencies via zypper..."
        sudo zypper install -y gtk4-devel libadwaita-devel libX11-devel libXtst-devel wl-clipboard xdotool pkg-config gcc curl git
    else
        warn "Could not identify package manager automatically. Ensure GTK4, Libadwaita, and X11 dev headers are installed."
    fi
}

# Auto-install dependencies if pkg-config cannot find gtk4 or libadwaita-1
if ! pkg-config --exists gtk4 libadwaita-1 2>/dev/null; then
    install_dependencies
fi

# 3. Ensure Rust Toolchain is Available
if ! command -v cargo >/dev/null 2>&1; then
    if [[ -f "${HOME}/.cargo/bin/cargo" ]]; then
        export PATH="${HOME}/.cargo/bin:${PATH}"
    else
        info "Rust compiler not found. Installing Rust toolchain via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
        export PATH="${HOME}/.cargo/bin:${PATH}"
    fi
fi

# 4. Build Release Binaries
info "Building release binaries with Cargo (this may take a moment)..."
cd "${PROJECT_ROOT}"
cargo build --release --workspace

# 5. Create Target Directories
mkdir -p "${BIN_DIR}" "${DESKTOP_DIR}" "${METAINFO_DIR}" "${ICON_DIR}" "${SYSTEMD_USER_DIR}"

# 6. Install Binaries
info "Installing binaries into ${BIN_DIR}..."
cp -f "${PROJECT_ROOT}/target/release/clipboard-history" "${BIN_DIR}/"
cp -f "${PROJECT_ROOT}/target/release/clipboard-history-daemon" "${BIN_DIR}/"
cp -f "${PROJECT_ROOT}/target/release/clipboard-history-gui" "${BIN_DIR}/"
chmod +x "${BIN_DIR}/clipboard-history"*

# Ensure BIN_DIR is in PATH
if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
    warn "${BIN_DIR} is not currently in your PATH. Consider adding:"
    warn "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    warn "to your ~/.bashrc or ~/.zshrc."
fi

# 7. Install Desktop Entry, Metainfo, and Icons
info "Installing desktop entry, AppStream metadata, and application icons..."
cp -f "${PROJECT_ROOT}/packaging/desktop/clipboard-history.desktop" "${DESKTOP_DIR}/"
cp -f "${PROJECT_ROOT}/packaging/desktop/clipboard-history.metainfo.xml" "${METAINFO_DIR}/"
cp -f "${PROJECT_ROOT}/packaging/desktop/icons/clipboard-history.svg" "${ICON_DIR}/"

for size in 16 24 32 48 64 128 256 512; do
    if [[ -f "${PROJECT_ROOT}/packaging/desktop/icons/hicolor/${size}x${size}/apps/clipboard-history.png" ]]; then
        mkdir -p "${HOME}/.local/share/icons/hicolor/${size}x${size}/apps"
        cp -f "${PROJECT_ROOT}/packaging/desktop/icons/hicolor/${size}x${size}/apps/clipboard-history.png" "${HOME}/.local/share/icons/hicolor/${size}x${size}/apps/"
    fi
done

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${DESKTOP_DIR}" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

# 8. Install and Start Systemd User Service
info "Configuring systemd background daemon service..."
cp -f "${PROJECT_ROOT}/packaging/systemd/clipboard-history.service" "${SYSTEMD_USER_DIR}/"

if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload
    systemctl --user enable clipboard-history.service
    systemctl --user restart clipboard-history.service
    success "Systemd user service enabled and started."
fi

# 9. Auto-configure Global Hotkey (Super+V) for GNOME
if command -v gsettings >/dev/null 2>&1 && [[ "${XDG_CURRENT_DESKTOP:-}" == *"GNOME"* ]]; then
    info "Configuring Super+V shortcut in GNOME Settings..."
    SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
    CUSTOM_KEY_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/"
    
    CURRENT_LIST=$(gsettings get "${SCHEMA}" custom-keybindings 2>/dev/null || echo "[]")
    if [[ "${CURRENT_LIST}" != *"${CUSTOM_KEY_PATH}"* ]]; then
        if [[ "${CURRENT_LIST}" == "@as []" ]] || [[ "${CURRENT_LIST}" == "[]" ]]; then
            NEW_LIST="['${CUSTOM_KEY_PATH}']"
        else
            NEW_LIST=$(echo "${CURRENT_LIST}" | sed "s/]$/, '${CUSTOM_KEY_PATH}']/")
        fi
        gsettings set "${SCHEMA}" custom-keybindings "${NEW_LIST}" || true
    fi

    CUSTOM_SCHEMA="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${CUSTOM_KEY_PATH}"
    gsettings set "${CUSTOM_SCHEMA}" name "Clipboard History" || true
    gsettings set "${CUSTOM_SCHEMA}" command "clipboard-history toggle" || true
    gsettings set "${CUSTOM_SCHEMA}" binding "<Super>v" || true
    success "GNOME Super+V shortcut configured automatically!"
fi

echo -e "\n${GREEN}${BOLD}Installation Complete!${NC}"
echo -e "Press ${BOLD}Super+V${NC} (or run ${BOLD}clipboard-history toggle${NC}) to open Clipboard History."
echo -e "All settings and preferences are fully configurable via the GUI Preferences dialog."
echo -e "Check daemon status anytime with: ${BOLD}clipboard-history status${NC}"
