#!/usr/bin/env bash
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info() { echo -e "${BLUE}${BOLD}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}${BOLD}[SUCCESS]${NC} $*"; }

BIN_DIR="${HOME}/.local/bin"
DESKTOP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
DATA_DIR="${HOME}/.local/share/clipboard-history"
CONFIG_DIR="${HOME}/.config/clipboard-history"

echo -e "${BOLD}========================================================${NC}"
echo -e "${BOLD}  Linux Universal Clipboard History Uninstaller (Win+V) ${NC}"
echo -e "${BOLD}========================================================${NC}"

# 1. Stop and Disable Systemd User Service
if command -v systemctl >/dev/null 2>&1; then
    info "Stopping systemd user service..."
    systemctl --user stop clipboard-history.service 2>/dev/null || true
    systemctl --user disable clipboard-history.service 2>/dev/null || true
    rm -f "${SYSTEMD_USER_DIR}/clipboard-history.service"
    systemctl --user daemon-reload || true
fi

# 2. Remove Binaries
info "Removing installed binaries..."
rm -f "${BIN_DIR}/clipboard-history"
rm -f "${BIN_DIR}/clipboard-history-daemon"
rm -f "${BIN_DIR}/clipboard-history-gui"

# 3. Remove Desktop Entry & Icons
info "Removing desktop entry and application icons..."
rm -f "${DESKTOP_DIR}/clipboard-history.desktop"
rm -f "${ICON_DIR}/clipboard-history.svg"
rm -f "${HOME}/.local/share/icons/hicolor/"*/apps/clipboard-history.png
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

# 4. Remove GNOME shortcut if present
if command -v gsettings >/dev/null 2>&1 && [[ "${XDG_CURRENT_DESKTOP:-}" == *"GNOME"* ]]; then
    SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
    CUSTOM_KEY_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/"
    CURRENT_LIST=$(gsettings get "${SCHEMA}" custom-keybindings 2>/dev/null || echo "[]")
    if [[ "${CURRENT_LIST}" == *"${CUSTOM_KEY_PATH}"* ]]; then
        CLEANED_LIST=$(echo "${CURRENT_LIST}" | sed "s/'${CUSTOM_KEY_PATH}', //; s/, '${CUSTOM_KEY_PATH}'//; s/'${CUSTOM_KEY_PATH}'//")
        gsettings set "${SCHEMA}" custom-keybindings "${CLEANED_LIST}" || true
    fi
fi

# 5. Purge Data & Config if requested
if [[ "${1:-}" == "--purge" ]]; then
    info "Purging database, blobs, and configuration..."
    rm -rf "${DATA_DIR}" "${CONFIG_DIR}"
    success "Data directory purged."
else
    echo -e "${YELLOW}History database and settings preserved at:${NC}"
    echo -e "  - ${DATA_DIR}"
    echo -e "  - ${CONFIG_DIR}"
    echo -e "Run with ${BOLD}--purge${NC} to completely wipe all history and configuration."
fi

success "Uninstallation completed successfully!"
