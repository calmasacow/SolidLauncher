//! The XDG autostart entry: a desktop file the session reads at login.

use std::path::{Path, PathBuf};

use solid_launcher_native::home_dir;

pub(super) fn entry_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".config/autostart/solid-launcher.desktop"))
}

fn entry(exe: &Path, minimized: bool) -> String {
    let mut exec = exe.display().to_string();
    // Exec is a command line, not a path: a space would read as a second argument.
    if exec.contains([' ', '\t', '"']) {
        exec = format!("\"{}\"", exec.replace('"', "\\\""));
    }
    if minimized {
        exec = format!("{exec} --hidden");
    }
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=SolidLauncher\n\
         Comment=Install, update and open SolidCraft\n\
         Exec={exec}\n\
         Icon=solid-launcher\n\
         Terminal=false\n\
         StartupNotify=false\n\
         StartupWMClass=solid-launcher\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

pub(super) fn write(path: &Path, exe: &Path, minimized: bool) -> Result<(), String> {
    std::fs::write(path, entry(exe, minimized))
        .map_err(|err| format!("could not write {} ({err})", path.display()))
}

#[cfg(test)]
mod tests {
    use super::entry;
    use std::path::Path;

    #[test]
    fn the_desktop_entry_carries_the_program_and_the_hidden_start() {
        let plain = entry(Path::new("/home/someone/bin/solid-launcher"), false);
        assert!(
            plain.contains("Exec=/home/someone/bin/solid-launcher\n"),
            "{plain}"
        );
        assert!(!plain.contains("--hidden"), "{plain}");

        let hidden = entry(Path::new("/home/someone/bin/solid-launcher"), true);
        assert!(
            hidden.contains("Exec=/home/someone/bin/solid-launcher --hidden\n"),
            "{hidden}"
        );

        // Exec is a command line: a path with a space has to be quoted or the session
        // reads the tail as a second argument.
        let spaced = entry(Path::new("/home/some one/bin/solid-launcher"), true);
        assert!(
            spaced.contains("Exec=\"/home/some one/bin/solid-launcher\" --hidden\n"),
            "{spaced}"
        );
    }
}
