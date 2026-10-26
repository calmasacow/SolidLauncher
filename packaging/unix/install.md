## Install

Pick the tarball for your machine. All of them unpack into your home directory and do not ask
for a password.

| Machine | File |
|---|---|
| Linux, 64-bit Intel or AMD | `solid-launcher-@VERSION@-linux-x86_64.tar.gz` |
| Linux, 64-bit ARM | `solid-launcher-@VERSION@-linux-aarch64.tar.gz` |

`uname -m` prints your architecture: `x86_64` or `aarch64`/`arm64`.

Download that file and `SHA256SUMS.txt` into the same folder, then:

```sh
sha256sum --ignore-missing -c SHA256SUMS.txt
mkdir -p ~/.local
tar -C ~/.local --strip-components=1 -xzf solid-launcher-@VERSION@-linux-x86_64.tar.gz
```

The check prints `OK` when the tarball
matches. The unpack puts `solid-launcher` in `~/.local/bin`, with a desktop entry and icons
under `~/.local/share`.

`~/.local/bin` has to be on `PATH`. If `command -v solid-launcher` prints nothing, add this
line to your shell startup file and open a new terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Start it:

```sh
solid-launcher
```

The window opens, and an icon stays in the status bar. The × hides the window. Quit is in the
icon's menu. The first launch turns on Start on login and Start minimized, so the next login
stays in the bar until you click the icon.

SolidCraft is not in this tarball. **Update** in the window installs it.
