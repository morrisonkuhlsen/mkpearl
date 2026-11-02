#!/bin/sh
# Builds the Debian package: build-deb.sh <version> <folder with pearl and pearl.png> <output.deb>
set -eu

version=$1 from=$2 out=$3
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

install -Dm755 "$from/pearl" "$root/usr/bin/pearl"
install -Dm644 "$from/pearl.png" "$root/usr/share/icons/hicolor/128x128/apps/pearl.png"
install -Dm644 packaging/linux/pearl.desktop "$root/usr/share/applications/pearl.desktop"
for file in LICENSE-MIT LICENSE-APACHE NOTICE ATTRIBUTION.md THIRD-PARTY-LICENSES.txt; do
    install -Dm644 "$file" "$root/usr/share/doc/pearl/$file"
done
mkdir -p "$root/DEBIAN"
cat > "$root/DEBIAN/control" <<CONTROL
Package: pearl
Version: $version
Section: editors
Priority: optional
Architecture: amd64
Depends: libc6, libssl3 | libssl3t64, libxkbcommon0, libgl1 | libegl1
Recommends: xdg-desktop-portal
Maintainer: Morrison Kühlsen
Homepage: https://github.com/morrisonkuhlsen/mkpearl
Description: Typst document editor
 Source on the left, live pages on the right, a ribbon on top, real-time
 sessions, version history and spell checking.
CONTROL
dpkg-deb --root-owner-group --build "$root" "$out"
