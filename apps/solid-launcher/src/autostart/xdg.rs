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
