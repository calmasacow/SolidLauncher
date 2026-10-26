# SolidLauncher

<p align="center">
  <img src="docs/screenshot.png" width="609" alt="SolidLauncher, with SolidCraft 0.23.0 up to date">
</p>

SolidLauncher downloads, installs, updates, and opens [SolidCraft](https://solidcraft.io). It reads the latest release from [GitHub](https://github.com/tangency-labs/solidcraft), picks the Linux build for this machine, and checks the file against the SHA-256 published with that release before anything is unpacked.

The window opens floating at 600×400. Its × hides it and leaves an icon in the status bar. Quit is in that icon's menu.

## Install

The [latest release](https://github.com/calmasacow/SolidLauncher/releases/latest) is a Linux x86_64 tarball. It installs into your home directory and does not ask for a password.

Download `solid-launcher-<version>-linux-x86_64.tar.gz` and `SHA256SUMS.txt` into the same folder. Then:

```sh
sha256sum -c SHA256SUMS.txt
mkdir -p ~/.local
tar -C ~/.local --strip-components=1 -xzf solid-launcher-<version>-linux-x86_64.tar.gz
```

`sha256sum` prints `OK` when the tarball matches the published checksum. The unpack puts:

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

## Where SolidCraft is installed

The Linux package is the official tarball. It is unpacked into the home directory, the same layout as installing it by hand, and it does not ask for a password.

```text
~/.local/opt/solidcraft-<version>-linux-<arch>/
~/.local/bin/solidcraft
~/.local/bin/solidcraft-cli
~/.local/bin/solvecraft
~/.local/bin/solvecraft-cli
```

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
| Remove old versions | off | Deletes older `~/.local/opt/solidcraft-*` trees that contain `bin/solidcraft`. Never the copy linked from `~/.local/bin/solidcraft`, and never when SolidCraft was not installed through SolidLauncher. |

The tray icon is a StatusNotifierItem named `solid-launcher`. Left click shows or hides the window. The menu is Show SolidLauncher, Check for updates, and Quit.

## Build

Rust 1.85 or newer.

```sh
cargo run -p solid-launcher             # window, and an icon in the bar
cargo run -p solid-launcher -- --hidden # bar icon only
cargo run -p solid-launcher -- --check  # installed version and the latest release
cargo test --workspace
```

A release is published by pushing a tag such as `v0.1.0`. The tag has to match the version in `Cargo.toml`. The release page includes the install steps above.

## Licence

SolidLauncher is [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

The SolidCraft mark, the raster icon, and the window banner are SolidCraft's brand, used here only to identify that app. See [ATTRIBUTION.md](ATTRIBUTION.md). They are not covered by the licence above.
