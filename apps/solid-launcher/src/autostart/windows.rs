//! The Windows login entry: a shortcut in the user's Startup folder.
//!
//! Not implemented yet. Writing a `.lnk` needs a shortcut writer, which arrives with the
//! rest of the Windows install support; until then the option reports that rather than
//! claiming to have turned itself on.

use std::path::{Path, PathBuf};

pub(super) fn entry_path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA")
        .filter(|value| !value.is_empty())
        .ok_or("no APPDATA directory")?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft/Windows/Start Menu/Programs/Startup")
        .join("SolidLauncher.lnk"))
}

pub(super) fn write(_path: &Path, _exe: &Path, _minimized: bool) -> Result<(), String> {
    Err(
        "not supported yet: SolidLauncher cannot start itself at login on Windows. \
Put a shortcut to it in the Startup folder by hand for now."
            .to_string(),
    )
}
