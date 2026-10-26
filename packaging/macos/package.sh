#!/usr/bin/env bash
# Build and package SolidLauncher for macOS.
#
# Usage: packaging/macos/package.sh <version> [--skip-build]
# Writes dist/solid-launcher-<version>-macos-universal.zip, holding SolidLauncher.app.
#
# A zip rather than a disk image: the app is dragged to Applications either way, and a zip
# needs no hdiutil dance in CI. The bundle is what matters — LSUIElement only works inside
# one, and without it the launcher cannot be a status bar app.
#
# Signing is done when SIGN_IDENTITY is set, and notarization when NOTARY_PROFILE is too.
# Without them the build is ad-hoc signed, and the first start needs right-click then Open.
set -euo pipefail

VERSION=$1
SKIP_BUILD=0
[ "${2:-}" = "--skip-build" ] && SKIP_BUILD=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
DIST="$ROOT/dist"
WORK="${CARGO_TARGET_DIR:-$ROOT/target}/macos-package"
APP="$WORK/SolidLauncher.app"
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)

# Keep in sync with LSMinimumSystemVersion in Info.plist.in.
export MACOSX_DEPLOYMENT_TARGET=11.0

echo "==> SolidLauncher $VERSION for macOS (universal)"
if [ "$SKIP_BUILD" = 0 ]; then
  args=()
  for target in "${TARGETS[@]}"; do args+=(--target "$target"); done
  cargo build --release --locked -p solid-launcher "${args[@]}"
fi

rm -rf "$WORK"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

inputs=()
for target in "${TARGETS[@]}"; do
  inputs+=("${CARGO_TARGET_DIR:-$ROOT/target}/$target/release/solid-launcher")
done
lipo -create -output "$APP/Contents/MacOS/SolidLauncher" "${inputs[@]}"
lipo -info "$APP/Contents/MacOS/SolidLauncher"
chmod 755 "$APP/Contents/MacOS/SolidLauncher"

sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist.in >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"

# ---- icon ---------------------------------------------------------------------------------
ICONSET="$WORK/SolidLauncher.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  double=$((size * 2))
  sips -z "$size" "$size" assets/solidcraft-256.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z "$double" "$double" assets/solidcraft-256.png \
    --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/SolidLauncher.icns"
rm -rf "$ICONSET"

# ---- sign ---------------------------------------------------------------------------------
# The hardened runtime is required for notarization, and a timestamp is required for a
# signature to stay valid after the certificate expires.
if [ -n "${SIGN_IDENTITY:-}" ]; then
  echo "==> signing as $SIGN_IDENTITY"
  codesign --force --deep --options runtime --timestamp \
    --sign "$SIGN_IDENTITY" "$APP"
else
  echo "==> no SIGN_IDENTITY: ad-hoc signing"
  codesign --force --deep --sign - "$APP"
fi
codesign --verify --strict --verbose=2 "$APP"

mkdir -p "$DIST"
ZIP="$DIST/solid-launcher-$VERSION-macos-universal.zip"
rm -f "$ZIP"
# ditto, not zip: it keeps the bundle's symlinks and permissions, and is what notarytool
# expects to be handed.
ditto -c -k --keepParent "$APP" "$ZIP"

# ---- notarize -----------------------------------------------------------------------------
if [ -n "${SIGN_IDENTITY:-}" ] && [ -n "${NOTARY_PROFILE:-}" ]; then
  echo "==> notarizing"
  xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
  # The ticket is stapled to the app, then the zip is rebuilt around the stapled copy:
  # a zip itself cannot be stapled.
  xcrun stapler staple "$APP"
  xcrun stapler validate "$APP"
  rm -f "$ZIP"
  ditto -c -k --keepParent "$APP" "$ZIP"
else
  echo "==> not notarized: the first start needs right-click then Open"
fi

echo "wrote $ZIP"
shasum -a 256 "$ZIP"
