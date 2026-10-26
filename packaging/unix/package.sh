#!/bin/sh
# Pack a Linux or FreeBSD release of SolidLauncher.
#
# Usage: packaging/unix/package.sh <version> <os> <arch> <binary>
# Writes dist/solid-launcher-<version>-<os>-<arch>.tar.gz
# The archive unpacks into ~/.local with --strip-components=1.
#
# Linux and the BSDs share this layout: the same XDG tree under the home directory, so one
# script serves both. Icons are rendered on the packaging host, which is always Linux.
set -eu

version=$1
os=$2
arch=$3
binary=$4

case "$os" in
    linux | freebsd) ;;
    *) echo "unknown os: $os (expected linux or freebsd)" >&2; exit 2 ;;
esac

cd "$(dirname "$0")/../.."

name="solid-launcher-${version}-${os}-${arch}"
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
# A cross-built or FreeBSD binary is stripped where it was built; host strip may not know it.
strip --strip-unneeded "$stage/bin/solid-launcher" 2>/dev/null || true
install -m 644 packaging/unix/solid-launcher.desktop \
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
