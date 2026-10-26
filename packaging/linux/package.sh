#!/bin/sh
# Pack a Linux release of SolidLauncher.
#
# Usage: packaging/linux/package.sh <version> <arch> <binary>
# Writes dist/solid-launcher-<version>-linux-<arch>.tar.gz
# The archive unpacks into ~/.local with --strip-components=1.
set -eu

version=$1
arch=$2
binary=$3

cd "$(dirname "$0")/../.."

name="solid-launcher-${version}-linux-${arch}"
stage="dist/${name}"

rm -rf "$stage"
mkdir -p \
    "$stage/bin" \
    "$stage/share/applications" \
    "$stage/share/icons/hicolor/48x48/apps" \
    "$stage/share/icons/hicolor/64x64/apps" \
    "$stage/share/icons/hicolor/128x128/apps" \
    "$stage/share/icons/hicolor/256x256/apps"

install -m 755 "$binary" "$stage/bin/solid-launcher"
strip --strip-unneeded "$stage/bin/solid-launcher"
install -m 644 packaging/linux/solid-launcher.desktop \
    "$stage/share/applications/solid-launcher.desktop"

for size in 48 64 128 256; do
    rsvg-convert -w "$size" -h "$size" assets/solidcraft.svg \
        -o "$stage/share/icons/hicolor/${size}x${size}/apps/solid-launcher.png"
done

tar -C dist -czf "dist/${name}.tar.gz" "$name"
rm -rf "$stage"

(
    cd dist
    sha256sum "${name}.tar.gz"
)
