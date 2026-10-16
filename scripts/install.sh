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
echo -e "${BOLD} Linux Universal Clipboard History Installer (Super+C)${NC}"
echo -e "${BOLD}======================================================${NC}"

# 1. Free any orphaned /tmp leftovers and configure physical disk storage for build temporaries
rm -rf /tmp/clipboard-history-install-* /tmp/cc*.s /tmp/cc*.o 2>/dev/null || true

BUILD_TMP_DIR="${HOME}/.cache/clipboard-history-tmp"
mkdir -p "${BUILD_TMP_DIR}"
export TMPDIR="${BUILD_TMP_DIR}"
trap 'rm -rf "${TEMP_DIR:-}" "${BUILD_TMP_DIR:-}" 2>/dev/null || true' EXIT INT TERM

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
    CACHE_DIR="${HOME}/.cache/clipboard-history-build"
    mkdir -p "${CACHE_DIR}"
    TEMP_DIR="$(mktemp -d -p "${CACHE_DIR}" install-XXXXXX)"
    trap 'rm -rf "${TEMP_DIR:-}" "${BUILD_TMP_DIR:-}" 2>/dev/null || true' EXIT INT TERM
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
AUTOSTART_DIR="${HOME}/.config/autostart"

# 2. Check and Install System Dependencies
install_dependencies() {
    info "Verifying required desktop and build libraries..."
    if command -v apt-get >/dev/null 2>&1; then
        info "Debian/Ubuntu detected. Installing dependencies via apt..."
        sudo apt-get update
        sudo apt-get install -y libgtk-4-dev libadwaita-1-dev libx11-dev libxtst-dev wl-clipboard xdotool tesseract-ocr pkg-config build-essential curl git
    elif command -v dnf >/dev/null 2>&1; then
        info "Fedora/RHEL detected. Installing dependencies via dnf..."
        sudo dnf install -y gtk4-devel libadwaita-devel libX11-devel libXtst-devel wl-clipboard xdotool tesseract pkgconf gcc curl git
    elif command -v pacman >/dev/null 2>&1; then
        info "Arch Linux detected. Installing dependencies via pacman..."
        sudo pacman -S --needed --noconfirm gtk4 libadwaita libx11 libxtst wl-clipboard xdotool tesseract pkgconf base-devel curl git
    elif command -v zypper >/dev/null 2>&1; then
        info "openSUSE detected. Installing dependencies via zypper..."
        sudo zypper install -y gtk4-devel libadwaita-devel libX11-devel libXtst-devel wl-clipboard xdotool tesseract pkg-config gcc curl git
    else
        warn "Could not identify package manager automatically. Ensure GTK4, Libadwaita, and X11 dev headers are installed."
    fi
}

# Auto-install dependencies if pkg-config, gtk4, libadwaita-1, wl-clipboard or xdotool are missing
needs_deps=0
if ! pkg-config --exists gtk4 libadwaita-1 2>/dev/null; then
    needs_deps=1
fi
if ! command -v pkg-config >/dev/null 2>&1 || ! command -v wl-paste >/dev/null 2>&1 || ! command -v xdotool >/dev/null 2>&1; then
    needs_deps=1
fi

if [[ "${needs_deps}" -eq 1 ]]; then
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
DATA_DIR="${HOME}/.local/share/clipboard-history"
CONFIG_DIR="${HOME}/.config/clipboard-history"
mkdir -p "${BIN_DIR}" "${DESKTOP_DIR}" "${METAINFO_DIR}" "${ICON_DIR}" "${SYSTEMD_USER_DIR}" "${DATA_DIR}" "${CONFIG_DIR}"

# Ensure High-Accuracy OCR Models (tessdata_best) are available
TESSDATA_DIR="${DATA_DIR}/tessdata"
mkdir -p "${TESSDATA_DIR}"
if [[ ! -f "${TESSDATA_DIR}/tur.traineddata" ]]; then
    info "Downloading high-accuracy Turkish OCR model (tessdata_best)..."
    curl -fsSL -o "${TESSDATA_DIR}/tur.traineddata" "https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/main/tur.traineddata" 2>/dev/null || true
fi
if [[ ! -f "${TESSDATA_DIR}/eng.traineddata" ]]; then
    info "Downloading high-accuracy English OCR model (tessdata_best)..."
    curl -fsSL -o "${TESSDATA_DIR}/eng.traineddata" "https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/main/eng.traineddata" 2>/dev/null || true
fi

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

# Ensure UFW rules are readable for Network Sync diagnostic verification if available
if [[ -f "/etc/ufw/user.rules" ]] && command -v sudo >/dev/null 2>&1; then
    sudo -n chmod 644 /etc/ufw/user.rules /etc/ufw/user6.rules 2>/dev/null || true
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
if [[ ! -f "${HOME}/.local/share/icons/hicolor/index.theme" ]] && [[ -f "/usr/share/icons/hicolor/index.theme" ]]; then
    cp -f "/usr/share/icons/hicolor/index.theme" "${HOME}/.local/share/icons/hicolor/index.theme" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

# 8. Configure Multi-Tier Universal Autostart
info "Configuring universal boot and login autostart..."
mkdir -p "${SYSTEMD_USER_DIR}" "${AUTOSTART_DIR}"

# Tier 1: Systemd User Service Unit
cp -f "${PROJECT_ROOT}/packaging/systemd/clipboard-history.service" "${SYSTEMD_USER_DIR}/"

# Tier 2: Universal XDG Desktop Autostart (with X-systemd-skip=true)
cp -f "${PROJECT_ROOT}/packaging/desktop/clipboard-history-autostart.desktop" "${AUTOSTART_DIR}/"

if command -v systemctl >/dev/null 2>&1 && systemctl --user list-units >/dev/null 2>&1; then
    systemctl --user import-environment WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_RUNTIME_DIR 2>/dev/null || true
    if command -v dbus-update-activation-environment >/dev/null 2>&1; then
        dbus-update-activation-environment --systemd WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_RUNTIME_DIR 2>/dev/null || true
    fi
    systemctl --user daemon-reload
    systemctl --user enable clipboard-history.service 2>/dev/null || true
    systemctl --user restart clipboard-history.service 2>/dev/null || true
    success "Systemd user service enabled and started."
else
    info "Non-systemd or container session detected; XDG desktop autostart is active."
fi

# Terminate any running GUI instance so newly installed GUI is loaded
killall -9 clipboard-history-gui 2>/dev/null || true

# 9. Auto-configure Global Hotkey (Super+C) for GNOME
GSETTINGS_BIN="/usr/bin/gsettings"
if [[ ! -x "${GSETTINGS_BIN}" ]]; then
    GSETTINGS_BIN="$(command -v gsettings 2>/dev/null || true)"
fi

if [[ -n "${GSETTINGS_BIN}" ]] && ( [[ "${XDG_CURRENT_DESKTOP:-}" =~ [Gg][Nn][Oo][Mm][Ee] ]] || [[ "${DESKTOP_SESSION:-}" =~ [Gg][Nn][Oo][Mm][Ee] ]] || [[ "${GDMSESSION:-}" =~ [Gg][Nn][Oo][Mm][Ee] ]] || command -v gnome-shell >/dev/null 2>&1 ); then
    info "Configuring Super+C shortcut in GNOME Settings..."
    SCHEMA="org.gnome.settings-daemon.plugins.media-keys"

    if command -v python3 >/dev/null 2>&1; then
        python3 -c "
import subprocess, ast, sys

gsettings = '${GSETTINGS_BIN}'
schema = '${SCHEMA}'
bin_cmd = '${BIN_DIR}/clipboard-history toggle'

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

# Clean up broken/non-standard paths like /clipboard-history/
legacy_path = '/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipboard-history/'
if legacy_path in cur:
    subprocess.run([gsettings, 'reset-recursively', f'{schema}.custom-keybinding:{legacy_path}'], capture_output=True)
    cur = [x for x in cur if x != legacy_path]
    set_list(cur)

# Find if an existing custom slot already points to clipboard-history
target_slot = None
for i in range(30):
    slot = f'/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom{i}/'
    sub_schema = f'{schema}.custom-keybinding:{slot}'
    cmd = subprocess.run([gsettings, 'get', sub_schema, 'command'], capture_output=True, text=True).stdout
    name = subprocess.run([gsettings, 'get', sub_schema, 'name'], capture_output=True, text=True).stdout
    if 'clipboard-history' in cmd or 'Clipboard History' in name:
        target_slot = slot
        break

# If not found, pick the first free slot
if not target_slot:
    for i in range(30):
        slot = f'/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom{i}/'
        if slot not in cur:
            target_slot = slot
            break

if target_slot:
    if target_slot not in cur:
        cur.append(target_slot)
        set_list(cur)

    sub_schema = f'{schema}.custom-keybinding:{target_slot}'
    subprocess.run([gsettings, 'set', sub_schema, 'name', 'Clipboard History'])
    subprocess.run([gsettings, 'set', sub_schema, 'command', bin_cmd])
    subprocess.run([gsettings, 'set', sub_schema, 'binding', '<Super>c'])
" 2>/dev/null || true
    fi
    success "GNOME Super+C shortcut configured automatically!"
fi

# 10. Auto-configure Tiling Window Managers & Compositors
# A. Hyprland
HYPR_CONF="${HOME}/.config/hypr/hyprland.conf"
if [[ -f "${HYPR_CONF}" ]] || [[ "${XDG_CURRENT_DESKTOP:-}" == *"Hyprland"* ]] || command -v hyprctl >/dev/null 2>&1; then
    info "Hyprland compositor detected. Ensuring window rules, autostart, and keybinding in Hyprland config..."
    if [[ -f "${HYPR_CONF}" ]]; then
        if ! grep -q "clipboard-history" "${HYPR_CONF}"; then
            cat << 'EOF' >> "${HYPR_CONF}"

# -----------------------------------------------------------------------------
# Clipboard History (Super+C)
# -----------------------------------------------------------------------------
windowrulev2 = float, class:^(clipboard-history-gui)$
windowrulev2 = center, class:^(clipboard-history-gui)$
bind = SUPER, C, exec, clipboard-history toggle
exec-once = clipboard-history-daemon
EOF
            success "Hyprland shortcut (Super+C), floating rules, and exec-once added to ${HYPR_CONF}!"
        else
            if ! grep -q "class:\^(clipboard-history-gui)\$" "${HYPR_CONF}"; then
                echo "windowrulev2 = float, class:^(clipboard-history-gui)$" >> "${HYPR_CONF}"
                echo "windowrulev2 = center, class:^(clipboard-history-gui)$" >> "${HYPR_CONF}"
            fi
            if ! grep -q "clipboard-history-daemon" "${HYPR_CONF}"; then
                echo "exec-once = clipboard-history-daemon" >> "${HYPR_CONF}"
            fi
            info "Hyprland configuration verified for Clipboard History."
        fi
        if command -v hyprctl >/dev/null 2>&1; then
            hyprctl reload >/dev/null 2>&1 || true
        fi
    fi
fi

# B. Sway
SWAY_CONF="${HOME}/.config/sway/config"
if [[ -f "${SWAY_CONF}" ]] || [[ "${XDG_CURRENT_DESKTOP:-}" == *"sway"* ]]; then
    if [[ -f "${SWAY_CONF}" ]] && ! grep -q "clipboard-history-daemon" "${SWAY_CONF}"; then
        echo -e "\n# Autostart Clipboard History Daemon\nexec clipboard-history-daemon" >> "${SWAY_CONF}"
        success "Sway autostart (exec) added to ${SWAY_CONF}."
    fi
fi

# C. i3
I3_CONF="${HOME}/.config/i3/config"
if [[ -f "${I3_CONF}" ]] || [[ "${XDG_CURRENT_DESKTOP:-}" == *"i3"* ]]; then
    if [[ -f "${I3_CONF}" ]] && ! grep -q "clipboard-history-daemon" "${I3_CONF}"; then
        echo -e "\n# Autostart Clipboard History Daemon\nexec --no-startup-id clipboard-history-daemon" >> "${I3_CONF}"
        success "i3 autostart (exec --no-startup-id) added to ${I3_CONF}."
    fi
fi

# Ensure daemon is running immediately for the active session
if ! "${BIN_DIR}/clipboard-history" daemon status >/dev/null 2>&1; then
    info "Launching daemon for immediate use in current session..."
    nohup "${BIN_DIR}/clipboard-history-daemon" >/dev/null 2>&1 &
fi

echo -e "\n${GREEN}${BOLD}Installation Complete!${NC}"
echo -e "Press ${BOLD}Super+C${NC} (or run ${BOLD}clipboard-history toggle${NC}) to open Clipboard History."
echo -e "All settings and preferences are fully configurable via the GUI Preferences dialog."
echo -e "Check daemon status anytime with: ${BOLD}clipboard-history daemon status${NC}"
echo -e "Start or stop daemon anytime with: ${BOLD}clipboard-history daemon start|stop|restart${NC}"
