#!/bin/bash
# Builds what people download: Tidy as a universal app (Apple silicon and Intel), packed as a
# .dmg with a drag-to-Applications window, plus a .zip and SHA-256 sums, in dist/.
# Usage: scripts/package-mac.sh [--arm64-only]
# The app is signed by scripts/bundle-mac.sh (Developer ID / Apple Development when installed,
# else ad hoc). Notarization, when configured, happens in the release workflow.
set -euo pipefail
cd "$(dirname "$0")/.."
UNIVERSAL=1
[[ "${1:-}" == "--arm64-only" ]] && UNIVERSAL=0
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)

if [[ $UNIVERSAL == 1 ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  cargo build --release -p tidy --target aarch64-apple-darwin
  cargo build --release -p tidy --target x86_64-apple-darwin
  mkdir -p target/universal
  lipo -create -output target/universal/tidy target/aarch64-apple-darwin/release/tidy target/x86_64-apple-darwin/release/tidy
  BIN=target/universal/tidy; ARCH=universal
else
  cargo build --release -p tidy
  BIN=target/release/tidy; ARCH=arm64
fi
scripts/bundle-mac.sh --bin "$BIN"
lipo -info target/Tidy.app/Contents/MacOS/Tidy

mkdir -p dist
NAME="Tidy-$VERSION-macos-$ARCH"
rm -f "dist/$NAME.dmg" "dist/$NAME.zip"

# The window's background, sharp on Retina: both sizes in one TIFF.
tiffutil -cathidpicheck app/assets/dmg/background.png app/assets/dmg/background@2x.png -out target/dmg-background.tiff >/dev/null

STAGE=$(mktemp -d)
cp -R target/Tidy.app "$STAGE/"
if command -v create-dmg >/dev/null; then
  # Finder lays the window out: Tidy on the left, Applications on the right, over the arrow.
  create-dmg \
    --volname "Tidy" \
    --volicon app/assets/icon/Tidy.icns \
    --background target/dmg-background.tiff \
    --window-pos 200 160 --window-size 660 400 \
    --icon-size 112 --text-size 13 \
    --icon "Tidy.app" 180 215 --hide-extension "Tidy.app" \
    --app-drop-link 480 215 \
    --no-internet-enable \
    "dist/$NAME.dmg" "$STAGE" \
  || { echo "create-dmg failed; making a plain disk image"; rm -f "dist/$NAME.dmg"; FALLBACK=1; }
else
  FALLBACK=1
fi
if [[ "${FALLBACK:-0}" == 1 ]]; then
  ln -s /Applications "$STAGE/Applications"
  hdiutil create -volname "Tidy" -srcfolder "$STAGE" -ov -format UDZO "dist/$NAME.dmg" >/dev/null
fi
rm -rf "$STAGE"

# Sign the disk image with the same identity as the app, when there is one.
IDENTITY=${TIDY_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | grep -E '"(Developer ID Application|Apple Development)' | head -1 | awk '{print $2}' || true)}
if [[ -n "$IDENTITY" ]]; then codesign --force --sign "$IDENTITY" "dist/$NAME.dmg"; fi

ditto -c -k --keepParent target/Tidy.app "dist/$NAME.zip"
(cd dist && shasum -a 256 "$NAME.dmg" "$NAME.zip" > "$NAME.sha256")
ls -lh dist
