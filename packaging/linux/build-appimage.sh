#!/usr/bin/env bash
# Wraps the already-built psx-xperience binary into an AppImage.
# Usage: build-appimage.sh <path-to-xperience-binary> <version> <out-path>
set -euo pipefail

BIN="$1"
VERSION="$2"
OUT="$3"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

APPDIR="$WORK/PSX_Xperience.AppDir"
mkdir -p "$APPDIR/usr/bin"
cp "$BIN" "$APPDIR/usr/bin/psx-xperience"
chmod +x "$APPDIR/usr/bin/psx-xperience"
cp "$HERE/psx-xperience.desktop" "$APPDIR/psx-xperience.desktop"
cp "$HERE/psx-xperience.png" "$APPDIR/psx-xperience.png"
cat > "$APPDIR/AppRun" << 'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/psx-xperience" "$@"
EOF
chmod +x "$APPDIR/AppRun"

TOOL="$WORK/appimagetool.AppImage"
curl -fsSL -o "$TOOL" \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x "$TOOL"

rm -f "$OUT"
# --appimage-extract-and-run: runners often lack FUSE, this sidesteps it.
"$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT"
echo "wrote $OUT"
