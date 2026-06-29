#!/bin/bash
# Build ledbetter.app (universal x86_64 + aarch64) and a DMG.
#
# Usage:   [VERSION=0.1.0] scripts/build-app.sh
# Prereqs: brew install create-dmg
#          rustup target add x86_64-apple-darwin aarch64-apple-darwin
#
# Icons are optional: if resources/icons/icon_1024.png (and the smaller sizes)
# exist they are bundled, otherwise the app ships without a custom icon.
set -euo pipefail

VERSION="${VERSION:-$(date +%Y.%m.%d)}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NAME="ledbetter"
BUNDLE_ID="com.bluefishforsale.ledbetter"
APP="$PROJECT_DIR/dist/$NAME.app"

# --- Build universal binary ---
echo "==> Building universal binary (v$VERSION)..."
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin

mkdir -p "$PROJECT_DIR/dist"
lipo -create \
  "$PROJECT_DIR/target/x86_64-apple-darwin/release/$NAME" \
  "$PROJECT_DIR/target/aarch64-apple-darwin/release/$NAME" \
  -output "$PROJECT_DIR/dist/$NAME"

# --- Assemble app bundle ---
echo "==> Assembling $NAME.app..."
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$PROJECT_DIR/dist/$NAME" "$APP/Contents/MacOS/$NAME"
chmod +x "$APP/Contents/MacOS/$NAME"

# --- Optional icon ---
ICON_KEY=""
ICON_DIR="$PROJECT_DIR/resources/icons"
if [ -f "$ICON_DIR/icon_1024.png" ]; then
  echo "==> Building icon..."
  ICONSET="$(mktemp -d)/$NAME.iconset"
  mkdir -p "$ICONSET"
  for pair in "16:16x16" "32:16x16@2x" "32:32x32" "64:32x32@2x" \
              "128:128x128" "256:128x128@2x" "256:256x256" "512:256x256@2x" \
              "512:512x512" "1024:512x512@2x"; do
    cp "$ICON_DIR/icon_${pair%%:*}.png" "$ICONSET/icon_${pair##*:}.png"
  done
  iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/$NAME.icns"
  rm -rf "$(dirname "$ICONSET")"
  ICON_KEY="    <key>CFBundleIconFile</key>
    <string>$NAME</string>"
fi

# --- Info.plist ---
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>$NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleName</key>
    <string>$NAME</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
$ICON_KEY
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeExtensions</key>
            <array><string>ledbetter</string></array>
            <key>CFBundleTypeName</key>
            <string>ledbetter Show</string>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
        </dict>
    </array>
    <key>UTExportedTypeDeclarations</key>
    <array>
        <dict>
            <key>UTTypeIdentifier</key>
            <string>$BUNDLE_ID.show</string>
            <key>UTTypeConformsTo</key>
            <array><string>public.data</string></array>
            <key>UTTypeDescription</key>
            <string>ledbetter Show</string>
            <key>UTTypeTagSpecification</key>
            <dict>
                <key>public.filename-extension</key>
                <array><string>ledbetter</string></array>
            </dict>
        </dict>
    </array>
    <key>NSCameraUsageDescription</key>
    <string>ledbetter uses the camera to map your LED fixtures in real space.</string>
    <key>NSLocalNetworkUsageDescription</key>
    <string>ledbetter sends Art-Net and sACN to your lighting controllers.</string>
</dict>
</plist>
PLIST

# --- Ad-hoc sign ---
echo "==> Signing (ad-hoc)..."
codesign -s - --force --deep --identifier "$BUNDLE_ID" "$APP"

# --- DMG ---
echo "==> Creating DMG..."
DMG="$PROJECT_DIR/dist/$NAME.dmg"
rm -f "$DMG"
create-dmg \
  --volname "$NAME" \
  --window-size 600 400 \
  --icon-size 128 \
  --icon "$NAME.app" 150 210 \
  --app-drop-link 450 210 \
  "$DMG" "$APP"

echo "==> Done: $DMG"
