#!/bin/sh
# Installs Pearl for this user, with no administrator rights: the program, its icon and its menu entry.
#   ./install.sh              to install
#   ./install.sh --uninstall  to remove them; documents and settings stay
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
share="${XDG_DATA_HOME:-$HOME/.local/share}"

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$bin/pearl" "$share/applications/pearl.desktop" "$share/icons/hicolor/128x128/apps/pearl.png"
    echo "Pearl removed. Documents and settings were kept."
    exit 0
fi

mkdir -p "$bin" "$share/applications" "$share/icons/hicolor/128x128/apps"
# Beside the old one and then over it, so that a Pearl already open keeps running.
cp "$here/pearl" "$bin/.pearl.new"
chmod 755 "$bin/.pearl.new"
mv -f "$bin/.pearl.new" "$bin/pearl"
cp "$here/pearl.png" "$share/icons/hicolor/128x128/apps/pearl.png"
sed "s|^Exec=pearl|Exec=$bin/pearl|" "$here/pearl.desktop" > "$share/applications/pearl.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$share/icons/hicolor" || true
echo "Pearl installed in $bin/pearl."
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "To run it from a terminal too, add $bin to your PATH." ;;
esac
