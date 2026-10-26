## Install

Linux x86_64. This unpacks into your home directory and does not ask for a password.

Download both files from this release into the same folder, then:

```sh
sha256sum -c SHA256SUMS.txt
mkdir -p ~/.local
tar -C ~/.local --strip-components=1 -xzf solid-launcher-@VERSION@-linux-x86_64.tar.gz
```

`sha256sum` prints `OK` when the tarball matches. That puts `solid-launcher` on `~/.local/bin`, with a desktop entry and icons under `~/.local/share`.

`~/.local/bin` has to be on `PATH`. If `command -v solid-launcher` prints nothing, add this line to your shell startup file and open a new terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Start it:

```sh
solid-launcher
```

The window opens, and an icon stays in the status bar. The × hides the window. Quit is in the icon's menu. The first launch turns on Start on login and Start minimized, so the next login stays in the bar until you click the icon.

SolidCraft is not in this tarball. **Update** in the window installs it.
