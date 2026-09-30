#!/usr/bin/env bash
# Build the release binary and install it into /Applications/SNES
# Xperience.app — the "update the app on my Mac for testing" one-liner.
# Usage: scripts/install-macos.sh [version]   (version defaults to the
# workspace's Cargo version)
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:-$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)}"
APP="/Applications/PSX Xperience.app"

cargo build --release -p xperience-app --bin psx-xperience

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/psx-xperience "$APP/Contents/MacOS/psx-xperience"
cp packaging/macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
codesign --force --deep --sign - "$APP" 2>/dev/null

echo "instalado: $APP (v$VERSION)"
