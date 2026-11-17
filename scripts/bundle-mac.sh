#!/bin/bash
# Builds Tidy.app: the binary, icon, Info.plist and About-panel credits, signed.
# Usage: scripts/bundle-mac.sh [--debug] [--open]
#   --debug  a debug build as target/Tidy Dev.app (its own bundle id), for day-to-day work:
#            signed with the same identity, so macOS keeps its permissions across rebuilds.
set -euo pipefail
cd "$(dirname "$0")/.."
DEBUG=0; OPEN=0
for a in "$@"; do case "$a" in --debug) DEBUG=1;; --open) OPEN=1;; esac; done
if [[ $DEBUG == 1 ]]; then
  cargo build -p tidy; BIN=target/debug/tidy; APP="target/Tidy Dev.app"; ID=app.tidy.mac.dev; NAME="Tidy Dev"
else
  cargo build --release -p tidy; BIN=target/release/tidy; APP=target/Tidy.app; ID=app.tidy.mac; NAME=Tidy
fi
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
BUILD=$(git rev-list --count HEAD 2>/dev/null || echo 1)
COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo dev)
YEAR=$(date +%Y)

# Replace the binary in place when the bundle exists, so the app keeps its identity.
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Tidy"
cp app/assets/icon/Tidy.icns "$APP/Contents/Resources/Tidy.icns"
cat > "$APP/Contents/Resources/Credits.rtf" <<RTF
{\rtf1\ansi\deff0{\fonttbl{\f0\fnil .AppleSystemUIFont;}}
\f0\fs22\qc See what fills your Mac, and clean it up with the AI you already have.\par
\par
{\fs18 Native disk map, developer cleanup, and an assistant powered by the Claude Code installed on this Mac. It sees names and sizes, never file contents, and nothing leaves for the Trash without you.\par
\par
Built in Rust with Rune.\par
Build ${BUILD} (${COMMIT})}
}
RTF
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>${NAME}</string>
  <key>CFBundleDisplayName</key><string>${NAME}</string>
  <key>CFBundleIdentifier</key><string>${ID}</string>
  <key>CFBundleExecutable</key><string>Tidy</string>
  <key>CFBundleIconFile</key><string>Tidy</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${BUILD}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSHumanReadableCopyright</key><string>© ${YEAR} Tidy. All rights reserved.</string>
  <key>NSDesktopFolderUsageDescription</key><string>Tidy measures how much space your Desktop takes. It never reads what's inside your files.</string>
  <key>NSDocumentsFolderUsageDescription</key><string>Tidy measures how much space your Documents take. It never reads what's inside your files.</string>
  <key>NSDownloadsFolderUsageDescription</key><string>Tidy measures your Downloads and finds old installers. It never reads what's inside your files.</string>
  <key>NSRemovableVolumesUsageDescription</key><string>Tidy measures how much space this drive's folders take.</string>
  <key>NSNetworkVolumesUsageDescription</key><string>Tidy measures how much space this volume's folders take.</string>
</dict>
</plist>
PLIST
# Sign with a real identity when there is one: macOS ties privacy grants (Full Disk Access,
# folder access) to the signature. An ad-hoc one changes with every build, so each rebuild
# would ask again; a certificate's doesn't.
IDENTITY=${TIDY_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | grep -E '"(Developer ID Application|Apple Development)' | head -1 | awk '{print $2}')}
if [[ -n "$IDENTITY" ]]; then
  codesign --force --options runtime --sign "$IDENTITY" "$APP"
else
  echo "warning: no signing identity; signing ad hoc (permissions reset on each build)"
  codesign --force --sign - "$APP"
fi
echo "Built $APP"
if [[ $OPEN == 1 ]]; then open "$APP"; fi
