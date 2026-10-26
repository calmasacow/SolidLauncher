# SolidLauncher

<p align="center">
  <img src="docs/screenshot.png" width="609" alt="SolidLauncher, with SolidCraft 0.23.0 up to date">
</p>

SolidLauncher downloads, installs, updates, and opens [SolidCraft](https://solidcraft.io). It reads the latest release from [GitHub](https://github.com/tangency-labs/solidcraft), picks the build for this machine, and checks the file against the SHA-256 published with that release before anything is unpacked.

Linux, Windows and macOS are supported.

The window opens floating at 600×400. Its × hides it and leaves an icon in the status bar. Quit is in that icon's menu.

## Install

The [latest release](https://github.com/calmasacow/SolidLauncher/releases/latest) has a tarball per machine. Each installs into your home directory and does not ask for a password.

| Machine | File |
|---|---|
| Linux, 64-bit Intel or AMD | `.AppImage`, `.deb`, `.rpm` or `.tar.gz`, all `-linux-x86_64` |
| Linux, 64-bit ARM | the same four, `-linux-aarch64` |
| Windows, 64-bit Intel or AMD | `solid-launcher-<version>-windows-x64.msi` |
| Windows, ARM | `solid-launcher-<version>-windows-arm64.msi` |
| macOS, Intel or Apple silicon | `solid-launcher-<version>-macos-universal.zip` |

### Linux

Four formats, same program in each. `uname -m` prints your architecture.

| File | What it is |
|---|---|
| `.AppImage` | One file. Make it executable and run it; nothing is installed. |
| `.deb` | A system package for Debian and Ubuntu. Needs root. |
| `.rpm` | A system package for Fedora and openSUSE. Needs root. |
| `.tar.gz` | Unpacks into your home directory. No root. |

```sh
# AppImage
chmod +x solid-launcher-<version>-linux-x86_64.AppImage
./solid-launcher-<version>-linux-x86_64.AppImage

# Debian or Ubuntu
sudo apt install ./solid-launcher-<version>-linux-x86_64.deb

# Fedora or openSUSE
sudo dnf install ./solid-launcher-<version>-linux-x86_64.rpm
```

For the tarball, download it and `SHA256SUMS.txt` into the same folder. Then:

```sh
sha256sum --ignore-missing -c SHA256SUMS.txt
mkdir -p ~/.local
tar -C ~/.local --strip-components=1 -xzf solid-launcher-<version>-linux-x86_64.tar.gz
```

The check prints `OK` when the tarball matches the published checksum. The unpack puts:

```text
~/.local/bin/solid-launcher
~/.local/share/applications/solid-launcher.desktop
~/.local/share/icons/hicolor/256x256/apps/solid-launcher.png
```

The icon is also installed at 48, 64, and 128 pixels. `~/.local/bin` has to be on `PATH`. Check with `command -v solid-launcher`. If that prints nothing, add this line to your shell startup file and open a new terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Start it:

```sh
solid-launcher
```

The window opens, and an icon stays in the status bar. The × hides the window. Quit is in the icon's menu. The first launch turns on Start on login and Start minimized, so the next login stays in the bar until you click the icon. Change those under Options.

SolidCraft is not in this tarball. **Update** in the window installs it when you ask.

### Windows

Run the `.msi`. It installs for you alone, into `%LOCALAPPDATA%\\Programs\\SolidLauncher`, and never asks for administrator rights, so it works on a machine where you are not an admin. It adds a Start Menu entry and an entry in Add or remove programs.

Check the download first:

```powershell
Get-FileHash -Algorithm SHA256 solid-launcher-<version>-windows-x64.msi
```

Compare that against `SHA256SUMS.txt`. The first run shows a SmartScreen warning, because the installer is not signed: **More info** then **Run anyway**.

A `.zip` is published beside the `.msi` for anyone who would rather unpack it by hand, or whose machine will not run an installer. It holds the same `solid-launcher.exe`.

**What the installer checks.** It refuses to install on anything older than Windows 10 and says so, rather than installing something that will not start. It does *not* check for the Visual C++ runtime, because there is nothing to check: the program is linked with a static C runtime, so `VCRUNTIME140.dll` is not needed. The graphics driver cannot be checked by an installer at all, so the Windows build draws through DirectX 12 instead of OpenGL — that works even on a machine running Microsoft's Basic Display Adapter, whose OpenGL is too old for the renderer the other platforms use.

### macOS

Unpack the zip and drag `SolidLauncher.app` into your Applications folder. Check the download first:

```sh
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
```

SolidLauncher is a status bar app: it has no Dock icon, and its window opens from the bar icon. If the release was not signed, the first start needs a right-click on the app and then **Open**; see [docs/signing.md](docs/signing.md).

## Where SolidCraft is installed

On Linux, SolidCraft is published in four formats, and SolidLauncher can install any of them. **Options** has the choice.

| Method | Where it goes | Password |
|---|---|---|
| Tarball | `~/.local/opt/solidcraft-<version>-<os>-<arch>/`, with `solidcraft-cli` | no |
| AppImage | `~/.local/opt/solidcraft-<version>-<os>-<arch>.AppImage`, one file | no |
| System package (`.deb`) | `apt-get install`, for everyone on the machine | yes, through `pkexec` |
| System package (`.rpm`) | `rpm -U`, for everyone on the machine | yes, through `pkexec` |

**Automatic** is the default and does two things in order. If SolidCraft is already on the machine — installed by `apt`, `dnf`, Flatpak, or an earlier SolidLauncher — it keeps that format, so an update never quietly swaps one install for another. Otherwise it takes the tarball, because that is the one that needs no password.

The tarball and the AppImage both link into `~/.local/bin`:

```text
~/.local/bin/solidcraft
~/.local/bin/solidcraft-cli
~/.local/bin/solvecraft
~/.local/bin/solvecraft-cli
```

A `.solid-launcher-method` file in `~/.local/opt` records which format was used. A Flatpak install is found and opened with `flatpak run`, but not updated: use the software centre or `flatpak update` for that.

On Windows there are two ways, and **Options** chooses between them. The portable zip is the default, because it matches the promise the Linux install makes: one user, no password.

| Method | Where it goes | Administrator |
|---|---|---|
| Portable zip | `%LOCALAPPDATA%\Programs\SolidCraft\solidcraft-<version>-windows-<arch>\` | no |
| Installer (MSI) | `%ProgramFiles%\SolidCraft`, for everyone on the machine | yes, a UAC prompt |

The installer also registers the SolidCraft file types and an entry in Add or remove programs, which a portable copy cannot. A `method` file beside the install records which was used, so SolidLauncher knows where to look next time.

On macOS, `SolidCraft.app` is copied out of the release's disk image into `~/Applications`, which needs no password. One copy exists at a time and each update replaces it, so there is nothing to sweep up and **Remove old versions** is not shown. The version is read from the bundle, so a copy you dragged across yourself is found too.

SolidCraft's own disk image is ad-hoc signed and not notarized, so SolidLauncher clears the quarantine flag after the download matches the SHA-256 the release published. Without that, every update would need a right-click and **Open**.

The `solvecraft` names stay so a launcher pinned before SolidCraft 0.8 keeps working. A file already in `~/.local/bin` that is not a symlink is left alone.

Downloads are cached in `~/.cache/solid-launcher/downloads`. A cached file is reused only when its SHA-256 still matches the release.

## Updates

SolidLauncher checks once when it starts, then once an hour while it stays open. **Check again** in the window, and **Check for updates** in the tray menu, check immediately.

A newer release than the installed copy posts one desktop notification for that version. The next launch notifies again if that version is still waiting. Nothing installed does not notify, and a check that fails does not notify.

**Update** installs that release. Older version folders stay in place unless **Remove old versions** is on.

## Options

Options sits beside the status line. The three checkboxes are stored in `~/.config/solid-launcher/settings.json`.

| Option | Default | What it does |
|---|---|---|
| Start on login | on | Writes `~/.config/autostart/solid-launcher.desktop`. Turning it off removes that file. |
| Start minimized | on | Login starts with `--hidden`, so no window is mapped until the icon is clicked. |
| Install SolidCraft as | Automatic | Which of the four Linux formats an install or update uses. Linux only; elsewhere there is one format and the choice is not shown. |
| Remove old versions | off | Deletes older `~/.local/opt/solidcraft-*` trees that contain `bin/solidcraft`. Never the copy linked from `~/.local/bin/solidcraft`, and never when SolidCraft was not installed through SolidLauncher. |

The tray icon is a StatusNotifierItem named `solid-launcher`. Left click shows or hides the window. The menu is Show SolidLauncher, Check for updates, and Quit.

## Build

Rust 1.85 or newer.

Every platform is built and released from CI; see `.github/workflows/release.yml`. The test suite covers the Windows install layout and the archive guards on any host, so `cargo test` on Linux exercises them too.

```sh
cargo run -p solid-launcher             # window, and an icon in the bar
cargo run -p solid-launcher -- --hidden # bar icon only
cargo run -p solid-launcher -- --check  # installed version and the latest release
cargo test --workspace
```

## Test builds

Every branch and pull request produces installable builds, so a change can be tried on real hardware before it is released. Open the run under [Actions](https://github.com/calmasacow/SolidLauncher/actions), scroll to **Artifacts**, and download the one for your machine:

| Artifact | Holds |
|---|---|
| `test-build-linux-x86_64`, `…-aarch64` | `.tar.gz`, `.AppImage`, `.deb`, `.rpm` |
| `test-build-windows-x64`, `…-arm64` | `.msi` and the portable `.zip` |
| `test-build-macos-universal` | `SolidLauncher.app` in a `.zip` |

These are test builds, not releases. They are versioned `0.0.0`, they are unsigned, and they are deleted after 14 days. GitHub wraps every artifact in a zip of its own, so unzip it first — and on Linux that drops the executable bit, which `chmod +x` puts back on the AppImage.

A release is published by pushing a tag such as `v0.1.0`. The tag has to match the version in `Cargo.toml`. The release page includes the install steps above.

## Licence

SolidLauncher is [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

The SolidCraft mark, the raster icon, and the window banner are SolidCraft's brand, used here only to identify that app. See [ATTRIBUTION.md](ATTRIBUTION.md). They are not covered by the licence above.
