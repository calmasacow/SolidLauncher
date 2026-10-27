//! Says why the launcher could not start, on a machine with nowhere to print it.
//!
//! A release build on Windows is a window application with no console, and on every
//! platform the usual way in is a Start Menu entry or a desktop file rather than a
//! terminal. So a failure to open the window used to look like nothing at all: the bar
//! icon appeared for a moment and the process was gone. This writes the reason to a file
//! and posts a notification pointing at it.

use std::path::PathBuf;

use solid_launcher_native::home_dir;

/// Where the reason is written. Beside the settings, so there is one folder to look in.
pub fn log_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".config/solid-launcher/last-error.log"))
}

/// Record `err` and tell the person where it went.
///
/// Every step here is best-effort: this runs when something has already gone wrong, and
/// failing to report a failure must not replace it with a different one.
pub fn report(err: &str) {
    eprintln!("SolidLauncher: {err}");
    let written = write(err);
    let note = match &written {
        Ok(path) => format!("{err}\n\nWritten to {}", path.display()),
        Err(_) => err.to_string(),
    };
    crate::notify::show("SolidLauncher could not start", &note);
}

fn write(err: &str) -> Result<PathBuf, String> {
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|fault| format!("could not make {} ({fault})", parent.display()))?;
    }
    let body = format!(
        "SolidLauncher {} on {}\n{err}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    std::fs::write(&path, body)
        .map_err(|fault| format!("could not write {} ({fault})", path.display()))?;
    Ok(path)
}
