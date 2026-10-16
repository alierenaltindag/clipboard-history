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
METAINFO_DIR="${HOME}/.local/share/metainfo"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
AUTOSTART_DIR="${HOME}/.config/autostart"
DATA_DIR="${HOME}/.local/share/clipboard-history"
CONFIG_DIR="${HOME}/.config/clipboard-history"

echo -e "${BOLD}========================================================${NC}"
echo -e "${BOLD} Linux Universal Clipboard History Uninstaller (Super+C)${NC}"
echo -e "${BOLD}========================================================${NC}"

# 1. Stop and Disable Systemd User Service & Remove Autostart
if command -v systemctl >/dev/null 2>&1; then
    info "Stopping systemd user service..."
    systemctl --user stop clipboard-history.service 2>/dev/null || true
    systemctl --user disable clipboard-history.service 2>/dev/null || true
    rm -f "${SYSTEMD_USER_DIR}/clipboard-history.service"
    systemctl --user daemon-reload || true
fi

# Remove XDG autostart entry
rm -f "${AUTOSTART_DIR}/clipboard-history-autostart.desktop"

# Ensure all processes are killed
pkill -f "clipboard-history" 2>/dev/null || true
rm -f "/run/user/${UID}/clipboard-history.sock" 2>/dev/null || true

# 2. Remove Binaries
info "Removing installed binaries..."
rm -f "${BIN_DIR}/clipboard-history"*

# 3. Remove Desktop Entry, Metainfo & Icons
info "Removing desktop entry, AppStream metadata, and application icons..."
rm -f "${DESKTOP_DIR}/clipboard-history.desktop"
rm -f "${METAINFO_DIR}/clipboard-history.metainfo.xml"
rm -f "${ICON_DIR}/clipboard-history.svg"
rm -f "${HOME}/.local/share/icons/hicolor/"*/apps/clipboard-history.png
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

# 4. Remove GNOME shortcut if present
GSETTINGS_BIN="/usr/bin/gsettings"
if [[ ! -x "${GSETTINGS_BIN}" ]]; then
    GSETTINGS_BIN="$(command -v gsettings 2>/dev/null || true)"
fi

if [[ -n "${GSETTINGS_BIN}" ]] && ( [[ "${XDG_CURRENT_DESKTOP:-}" =~ [Gg][Nn][Oo][Mm][Ee] ]] || [[ "${DESKTOP_SESSION:-}" =~ [Gg][Nn][Oo][Mm][Ee] ]] || [[ "${GDMSESSION:-}" =~ [Gg][Nn][Oo][Mm][Ee] ]] || command -v gnome-shell >/dev/null 2>&1 ); then
    info "Removing Clipboard History shortcuts from GNOME Settings..."
    SCHEMA="org.gnome.settings-daemon.plugins.media-keys"

    if command -v python3 >/dev/null 2>&1; then
        python3 -c "
import subprocess, ast

gsettings = '${GSETTINGS_BIN}'
schema = '${SCHEMA}'

def get_list():
    res = subprocess.run([gsettings, 'get', schema, 'custom-keybindings'], capture_output=True, text=True)
    raw = res.stdout.strip()
    if raw.startswith('@as '):
        raw = raw[4:]
    try:
        val = ast.literal_eval(raw)
        return val if isinstance(val, list) else []
    except Exception:
        return []

def set_list(lst):
    val = '@as []' if not lst else '[' + ', '.join(f\"'{x}'\" for x in lst) + ']'
    subprocess.run([gsettings, 'set', schema, 'custom-keybindings', val], check=True)

cur = get_list()
to_remove = set()

for i in range(30):
    slot = f'/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom{i}/'
    sub_schema = f'{schema}.custom-keybinding:{slot}'
    cmd = subprocess.run([gsettings, 'get', sub_schema, 'command'], capture_output=True, text=True).stdout
    name = subprocess.run([gsettings, 'get', sub_schema, 'name'], capture_output=True, text=True).stdout
    if 'clipboard-history' in cmd or 'Clipboard History' in name:
        subprocess.run([gsettings, 'reset-recursively', sub_schema], capture_output=True)
        to_remove.add(slot)

legacy_path = '/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/'
subprocess.run([gsettings, 'reset-recursively', f'{schema}.custom-keybinding:{legacy_path}'], capture_output=True)
to_remove.add(legacy_path)

new_cur = [x for x in cur if x not in to_remove]
if new_cur != cur:
    set_list(new_cur)
" 2>/dev/null || true
    fi
fi

# 5. Remove Tiling WM configs if present
for conf in "${HOME}/.config/hypr/hyprland.conf" "${HOME}/.config/sway/config" "${HOME}/.config/i3/config"; do
    if [[ -f "${conf}" ]] && grep -q "clipboard-history" "${conf}" 2>/dev/null; then
        info "Cleaning Clipboard History rules from ${conf}..."
        sed -i '/Clipboard History/d' "${conf}" 2>/dev/null || true
        sed -i '/clipboard-history/d' "${conf}" 2>/dev/null || true
    fi
done
if command -v hyprctl >/dev/null 2>&1; then
    hyprctl reload >/dev/null 2>&1 || true
fi

# 6. Purge Data & Config if requested
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
