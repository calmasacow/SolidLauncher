#!/bin/sh
# Pack a Linux or FreeBSD release of SolidLauncher.
#
# Usage: packaging/unix/package.sh <version> <os> <arch> <binary> [formats]
#   formats: a space-separated list of tar, appimage, deb, rpm.
#            Default: "tar appimage deb rpm" on Linux, "tar" on FreeBSD.
#
# Writes, into dist/:
#   solid-launcher-<version>-<os>-<arch>.tar.gz    unpacks into ~/.local, no root
#   solid-launcher-<version>-linux-<arch>.AppImage one file, no install at all
#   solid-launcher-<version>-linux-<arch>.deb      a system package, needs root
#   solid-launcher-<version>-linux-<arch>.rpm      a system package, needs root
#
# One FHS tree is staged and every format is built from it, so the four cannot drift
# apart. Needs: rsvg-convert for icons; nfpm for deb and rpm; appimagetool for the
# AppImage. Icons are rendered on the packaging host, which is always Linux.
set -eu

version=$1
os=$2
arch=$3
binary=$4
formats=${5:-}

case "$os" in
    linux) formats=${formats:-"tar appimage deb rpm"} ;;
    freebsd) formats=${formats:-"tar"} ;;
    *) echo "unknown os: $os (expected linux or freebsd)" >&2; exit 2 ;;
esac

cd "$(dirname "$0")/../.."
root=$(pwd)
name="solid-launcher-${version}-${os}-${arch}"
work="dist/.stage-${os}-${arch}"
stage="$work/root"

has() { case " $formats " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

rm -rf "$work"
mkdir -p "$stage/usr/bin" "$stage/usr/share/applications"
for size in 48 64 128 256; do
    mkdir -p "$stage/usr/share/icons/hicolor/${size}x${size}/apps"
done

install -m 755 "$binary" "$stage/usr/bin/solid-launcher"
# A cross-built or FreeBSD binary is stripped where it was built; host strip may not know it.
strip --strip-unneeded "$stage/usr/bin/solid-launcher" 2>/dev/null || true
install -m 644 packaging/unix/solid-launcher.desktop \
    "$stage/usr/share/applications/solid-launcher.desktop"
for size in 48 64 128 256; do
    rsvg-convert -w "$size" -h "$size" assets/solidcraft.svg \
        -o "$stage/usr/share/icons/hicolor/${size}x${size}/apps/solid-launcher.png"
done

# ---- .tar.gz -----------------------------------------------------------------------------
# Unpacked with --strip-components=1 into ~/.local, so the tree inside is bin/ and share/
# rather than usr/bin and usr/share.
if has tar; then
    mkdir -p "$work/tar/$name"
    cp -R "$stage/usr/." "$work/tar/$name/"
    tar -C "$work/tar" -czf "dist/${name}.tar.gz" "$name"
    echo "wrote dist/${name}.tar.gz"
fi

# ---- .deb and .rpm -----------------------------------------------------------------------
if has deb || has rpm; then
    command -v nfpm >/dev/null || {
        echo "error: nfpm not found (https://nfpm.goreleaser.com/install/)" >&2; exit 1; }
    # nfpm expands environment variables in `version` and `arch`, but not in contents[].src.
    sed "s|\${STAGE}|$root/$stage|g" packaging/unix/nfpm.yaml >"$work/nfpm.yaml"
    export SOLID_LAUNCHER_VERSION="$version"
    case "$arch" in
        x86_64) export SOLID_LAUNCHER_ARCH=amd64 ;;
        aarch64) export SOLID_LAUNCHER_ARCH=arm64 ;;
        *) echo "no deb/rpm architecture known for $arch" >&2; exit 2 ;;
    esac
    # An explicit target path, so the deb and rpm are named like every other artifact
    # rather than in nfpm's own two different styles.
    for fmt in deb rpm; do
        if has "$fmt"; then
            nfpm package -f "$work/nfpm.yaml" -p "$fmt" -t "dist/${name}.${fmt}"
            echo "wrote dist/${name}.${fmt}"
        fi
    done
fi

# ---- .AppImage ---------------------------------------------------------------------------
# An AppImage is its own install: one executable file that needs no root and no unpacking.
if has appimage; then
    command -v appimagetool >/dev/null || {
        echo "error: appimagetool not found (https://github.com/AppImage/AppImageKit)" >&2
        exit 1; }
    appdir="$work/AppDir"
    rm -rf "$appdir"
    cp -R "$stage" "$appdir"
    # The three things appimagetool looks for at the top of the AppDir.
    cp "$stage/usr/share/applications/solid-launcher.desktop" "$appdir/solid-launcher.desktop"
    cp "$stage/usr/share/icons/hicolor/256x256/apps/solid-launcher.png" \
        "$appdir/solid-launcher.png"
    cat >"$appdir/AppRun" <<'RUN'
#!/bin/sh
# Run the launcher from wherever the AppImage was mounted.
HERE=$(dirname "$(readlink -f "$0")")
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/solid-launcher" "$@"
RUN
    chmod 755 "$appdir/AppRun"
    case "$arch" in
        x86_64) appimage_arch=x86_64 ;;
        aarch64) appimage_arch=aarch64 ;;
        *) echo "no AppImage architecture known for $arch" >&2; exit 2 ;;
    esac
    ARCH="$appimage_arch" appimagetool --no-appstream "$appdir" \
        "dist/solid-launcher-${version}-${os}-${arch}.AppImage"
    echo "wrote dist/solid-launcher-${version}-${os}-${arch}.AppImage"
fi

rm -rf "$work"

(
    cd dist
    for f in solid-launcher-*"${version}"*; do
        [ -f "$f" ] && sha256sum "$f"
    done
)
