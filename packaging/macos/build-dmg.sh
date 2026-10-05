#!/usr/bin/env bash
# Wraps the already-built polystnx binary into PolystnX.app and a DMG.
# Usage: build-dmg.sh <path-to-polystnx-binary> <version> <out-dmg-path>
set -euo pipefail

BIN="$1"
VERSION="$2"
OUT_DMG="$3"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

APP="$WORK/PolystnX.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN" "$APP/Contents/MacOS/polystnx"
chmod +x "$APP/Contents/MacOS/polystnx"
cp "$HERE/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
sed "s/__VERSION__/$VERSION/g" "$HERE/Info.plist" > "$APP/Contents/Info.plist"

# Ad-hoc sign (identity "-", no Apple Developer account needed). Without
# this, Gatekeeper's message for an unsigned app that's been quarantined by
# a browser download is "is damaged and can't be opened" — misleading (the
# file isn't corrupt), but that's what it shows instead of the older
# "unidentified developer" prompt once there's no signature at all.
codesign --force --deep --sign - "$APP"

# Drag-to-install convenience: a shortcut to /Applications alongside the app.
ln -s /Applications "$WORK/Applications"

rm -f "$OUT_DMG"
hdiutil create -volname "PolystnX" -srcfolder "$WORK" -ov -format UDZO "$OUT_DMG"
echo "wrote $OUT_DMG"
