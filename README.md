# SolidLauncher

<p align="center">
  <img src="docs/screenshot.png" width="609" alt="SolidLauncher, with SolidCraft 0.23.0 up to date">
</p>

SolidLauncher downloads, installs, updates, and opens [SolidCraft](https://solidcraft.io). It reads the latest release from [GitHub](https://github.com/tangency-labs/solidcraft), picks the Linux build for this machine, and checks the file against the SHA-256 published with that release before anything is unpacked.

The window opens floating at 600×400. Its × hides it and leaves an icon in the status bar. Quit is in that icon's menu.

## Download

The [latest release](https://github.com/calmasacow/SolidLauncher/releases/latest) has a Linux tarball and `SHA256SUMS.txt`. Unpack it into your home directory:

```sh
tar -C ~/.local --strip-components=1 -xzf solid-launcher-<version>-linux-<arch>.tar.gz
```

That puts `solid-launcher` on `~/.local/bin`, and the icon and desktop entry under `~/.local/share`. `~/.local/bin` needs to be on `PATH`.

A release is published when a tag such as `v0.1.0` is pushed, and that tag has to match the version in `Cargo.toml`.

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

## Licence

SolidLauncher is [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

The SolidCraft mark, the raster icon, and the window banner are SolidCraft's brand, used here only to identify that app. See [ATTRIBUTION.md](ATTRIBUTION.md). They are not covered by the licence above.
