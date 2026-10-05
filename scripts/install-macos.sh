#!/usr/bin/env bash
# Build the release binary and install it into /Applications/PolystnX.app — the "update the app on my Mac for testing" one-liner.
# Usage: scripts/install-macos.sh [version]   (version defaults to the
# workspace's Cargo version)
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:-$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)}"
APP="/Applications/PolystnX.app"

# O rebrand mudou o nome do bundle — a instalação antiga sai do caminho.
if [ -d "/Applications/PSX Xperience.app" ] && [ -d "/Applications/PolystnX.app" ]; then
  rm -rf "/Applications/PSX Xperience.app"
fi

cargo build --release -p polystnx-app --bin polystnx

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/polystnx "$APP/Contents/MacOS/polystnx"
cp packaging/macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
codesign --force --deep --sign - "$APP" 2>/dev/null

echo "instalado: $APP (v$VERSION)"
