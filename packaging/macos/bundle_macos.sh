#!/usr/bin/env bash
set -euo pipefail

# Builds a portable, branded macOS BeatCheck.app bundle (run on macOS).
# Supports universal binary (Apple Silicon + Intel) if both rustup targets are installed.

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

APP_DIR="dist/BeatCheck.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp packaging/macos/Info.plist "$APP_DIR/Contents/Info.plist"

# Generate native macOS BeatCheck.icns from the Pulse Check 1024x1024 master icon
if [ -f "assets/brand/icon_1024.png" ] && command -v sips >/dev/null 2>&1 && command -v iconutil >/dev/null 2>&1; then
    ICONSET_DIR="$(mktemp -d)/BeatCheck.iconset"
    mkdir -p "$ICONSET_DIR"
    for sz in 16 32 128 256 512; do
        sips -z $sz $sz assets/brand/icon_1024.png --out "$ICONSET_DIR/icon_${sz}x${sz}.png" >/dev/null
        dbl=$((sz * 2))
        sips -z $dbl $dbl assets/brand/icon_1024.png --out "$ICONSET_DIR/icon_${sz}x${sz}@2x.png" >/dev/null
    done
    iconutil -c icns "$ICONSET_DIR" -o "$APP_DIR/Contents/Resources/BeatCheck.icns"
    rm -rf "$(dirname "$ICONSET_DIR")"
fi

if rustup target list --installed | grep -q "aarch64-apple-darwin" && \
   rustup target list --installed | grep -q "x86_64-apple-darwin"; then
    echo "Building Universal macOS binary (arm64 + x86_64)..."
    cargo build --release --target aarch64-apple-darwin -p beatcheck-app
    cargo build --release --target x86_64-apple-darwin -p beatcheck-app
    lipo -create \
        target/aarch64-apple-darwin/release/BeatCheck \
        target/x86_64-apple-darwin/release/BeatCheck \
        -output "$APP_DIR/Contents/MacOS/BeatCheck"
else
    echo "Building native host macOS binary..."
    cargo build --release -p beatcheck-app
    cp target/release/BeatCheck "$APP_DIR/Contents/MacOS/BeatCheck"
fi

chmod +x "$APP_DIR/Contents/MacOS/BeatCheck"
if command -v codesign >/dev/null 2>&1; then
    echo "Ad-hoc signing BeatCheck.app for Apple Silicon..."
    codesign --force --deep --sign - "$APP_DIR"
fi
echo "Portable macOS bundle created at: $APP_DIR"
