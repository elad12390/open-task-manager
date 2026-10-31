#!/usr/bin/env bash
# Build "Open Task Manager.app" and a drag-to-install DMG.
#
#   scripts/package.sh                 # host architecture only
#   UNIVERSAL=1 scripts/package.sh     # Apple silicon + Intel in one binary (used by releases)
#
# Output: dist/Open Task Manager.app and dist/OpenTaskManager-<version>.dmg
set -euo pipefail

cd "$(dirname "$0")/.."
APP_NAME="Open Task Manager"
BUNDLE_ID="org.opentaskmanager.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST="dist"
APP="$DIST/$APP_NAME.app"
DMG="$DIST/OpenTaskManager-$VERSION.dmg"

if [[ "${UNIVERSAL:-0}" == "1" ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  mkdir -p target/universal
  lipo -create -output target/universal/otm \
    target/aarch64-apple-darwin/release/otm target/x86_64-apple-darwin/release/otm
  BIN="target/universal/otm"
else
  cargo build --release
  BIN="target/release/otm"
fi

rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/otm"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>otm</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>Open source under the Apache License 2.0</string>
</dict>
</plist>
PLIST

# Ad-hoc signature: required for Apple silicon; not notarized (no Apple Developer ID).
codesign --force --deep --sign - "$APP"

STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"

echo "app: $APP"
echo "dmg: $DMG"
shasum -a 256 "$DMG"
