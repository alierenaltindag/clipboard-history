#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="0.1.0"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo "amd64")"
PACKAGE_NAME="clipboard-history_${VERSION}_${ARCH}"
BUILD_ROOT="${PROJECT_ROOT}/target/deb/${PACKAGE_NAME}"

echo "Building Debian package: ${PACKAGE_NAME}.deb..."
cargo build --release --workspace

rm -rf "${BUILD_ROOT}"
mkdir -p "${BUILD_ROOT}/DEBIAN"
mkdir -p "${BUILD_ROOT}/usr/bin"
mkdir -p "${BUILD_ROOT}/usr/share/applications"
mkdir -p "${BUILD_ROOT}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${BUILD_ROOT}/usr/lib/systemd/user"

# Control file
cat > "${BUILD_ROOT}/DEBIAN/control" << EOF
Package: clipboard-history
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Depends: libc6, libx11-6, libxtst6
Recommends: wl-clipboard, xdotool
Maintainer: Ali Eren Altındağ <alierenaltindaag@gmail.com>
Description: High-performance Universal Clipboard History utility for Linux
 Modern Win+V alternative for Wayland and X11 desktop environments,
 supporting rich text, images, file lists, and instant search.
EOF

# Post-install script
cat > "${BUILD_ROOT}/DEBIAN/postinst" << 'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
fi
exit 0
EOF
chmod 755 "${BUILD_ROOT}/DEBIAN/postinst"

# Binaries
cp -f "${PROJECT_ROOT}/target/release/clipboard-history" "${BUILD_ROOT}/usr/bin/"
cp -f "${PROJECT_ROOT}/target/release/clipboard-history-daemon" "${BUILD_ROOT}/usr/bin/"
cp -f "${PROJECT_ROOT}/target/release/clipboard-history-gui" "${BUILD_ROOT}/usr/bin/"
chmod 755 "${BUILD_ROOT}/usr/bin/"*

# Desktop, Icon, Systemd
cp -f "${PROJECT_ROOT}/packaging/desktop/clipboard-history.desktop" "${BUILD_ROOT}/usr/share/applications/"
cp -f "${PROJECT_ROOT}/packaging/desktop/icons/clipboard-history.svg" "${BUILD_ROOT}/usr/share/icons/hicolor/scalable/apps/"

for size in 16 24 32 48 64 128 256 512; do
    if [[ -f "${PROJECT_ROOT}/packaging/desktop/icons/hicolor/${size}x${size}/apps/clipboard-history.png" ]]; then
        mkdir -p "${BUILD_ROOT}/usr/share/icons/hicolor/${size}x${size}/apps"
        cp -f "${PROJECT_ROOT}/packaging/desktop/icons/hicolor/${size}x${size}/apps/clipboard-history.png" "${BUILD_ROOT}/usr/share/icons/hicolor/${size}x${size}/apps/"
    fi
done

cp -f "${PROJECT_ROOT}/packaging/systemd/clipboard-history.service" "${BUILD_ROOT}/usr/lib/systemd/user/"

dpkg-deb --build "${BUILD_ROOT}" "${PROJECT_ROOT}/target/${PACKAGE_NAME}.deb"
echo "Debian package successfully built at: ${PROJECT_ROOT}/target/${PACKAGE_NAME}.deb"
