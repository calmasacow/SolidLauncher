//! The Windows login entry: a shortcut in this user's Startup folder.
//!
//! Everything in that folder is started when the user signs in. A shortcut is used rather
//! than a `Run` registry value because the user can see it, move it and delete it, which
//! matches what the other platforms do with their own file.

use std::path::{Path, PathBuf};

use solid_launcher_native::write_shortcut;

pub(super) fn entry_path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA")
        .filter(|value| !value.is_empty())
        .ok_or("no APPDATA directory")?;
    Ok(PathBuf::from(appdata)
        .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
        .join("SolidLauncher.lnk"))
}

pub(super) fn write(path: &Path, exe: &Path, minimized: bool) -> Result<(), String> {
    write_shortcut(exe, path, minimized.then_some("--hidden"))
}
