#!/usr/bin/env bash
set -euo pipefail

# AppImage builder script for Linux Universal Clipboard History

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/target/appimage"
APP_DIR="${BUILD_DIR}/AppDir"

echo "Building AppImage package..."
cargo build --release --workspace

rm -rf "${APP_DIR}"
mkdir -p "${APP_DIR}/usr/bin"
mkdir -p "${APP_DIR}/usr/share/applications"
mkdir -p "${APP_DIR}/usr/share/icons/hicolor/scalable/apps"

cp -f "${PROJECT_ROOT}/target/release/clipboard-history" "${APP_DIR}/usr/bin/"
cp -f "${PROJECT_ROOT}/target/release/clipboard-history-daemon" "${APP_DIR}/usr/bin/"
cp -f "${PROJECT_ROOT}/target/release/clipboard-history-gui" "${APP_DIR}/usr/bin/"

cp -f "${PROJECT_ROOT}/packaging/desktop/clipboard-history.desktop" "${APP_DIR}/"
cp -f "${PROJECT_ROOT}/packaging/desktop/clipboard-history.desktop" "${APP_DIR}/usr/share/applications/"
cp -f "${PROJECT_ROOT}/packaging/desktop/icons/clipboard-history.svg" "${APP_DIR}/clipboard-history.svg"
cp -f "${PROJECT_ROOT}/packaging/desktop/icons/clipboard-history.svg" "${APP_DIR}/usr/share/icons/hicolor/scalable/apps/"
cp -f "${PROJECT_ROOT}/packaging/desktop/icons/clipboard-history.png" "${APP_DIR}/clipboard-history.png"
cp -f "${PROJECT_ROOT}/packaging/desktop/icons/clipboard-history.png" "${APP_DIR}/.DirIcon"

for size in 16 24 32 48 64 128 256 512; do
    if [[ -f "${PROJECT_ROOT}/packaging/desktop/icons/hicolor/${size}x${size}/apps/clipboard-history.png" ]]; then
        mkdir -p "${APP_DIR}/usr/share/icons/hicolor/${size}x${size}/apps"
        cp -f "${PROJECT_ROOT}/packaging/desktop/icons/hicolor/${size}x${size}/apps/clipboard-history.png" "${APP_DIR}/usr/share/icons/hicolor/${size}x${size}/apps/"
    fi
done

# Create AppRun entrypoint
cat > "${APP_DIR}/AppRun" << 'EOF'
#!/bin/sh
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH}"
exec "${HERE}/usr/bin/clipboard-history-gui" "$@"
EOF
chmod +x "${APP_DIR}/AppRun"

# Download and execute appimagetool if not available
if ! command -v appimagetool >/dev/null 2>&1; then
    echo "Downloading appimagetool..."
    curl -sLo /tmp/appimagetool "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    chmod +x /tmp/appimagetool
    TOOL="/tmp/appimagetool"
else
    TOOL="appimagetool"
fi

ARCH=x86_64 "${TOOL}" "${APP_DIR}" "${PROJECT_ROOT}/target/ClipboardHistory-x86_64.AppImage"
echo "AppImage created at ${PROJECT_ROOT}/target/ClipboardHistory-x86_64.AppImage"
