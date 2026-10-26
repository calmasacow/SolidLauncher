//! Start SolidLauncher when the user logs in.
//!
//! Every platform does this with one file that the user owns, so turning the option off is
//! a delete and nothing is left behind. What differs is where the file goes and what is in
//! it, which is all a backend provides.
//!
//! | Platform | File |
//! |---|---|
//! | Linux, the BSDs | `~/.config/autostart/solid-launcher.desktop` |
//! | macOS | `~/Library/LaunchAgents/<label>.plist` |
//! | Windows | a shortcut in the Startup folder |

use std::path::{Path, PathBuf};

use crate::settings::Settings;

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
mod xdg;
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
use xdg as backend;

// Compiled under cfg(test) everywhere, not only on macOS, so the plist it writes is
// covered by the suite wherever it runs. A LaunchAgent is a file, and nothing about
// writing one needs a Mac.
#[cfg(any(target_os = "macos", test))]
mod macos;
#[cfg(target_os = "macos")]
use macos as backend;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as backend;

/// Where this platform keeps the login entry.
pub fn entry_path() -> Result<PathBuf, String> {
    backend::entry_path()
}

/// Create or remove the login entry to match the options.
pub fn apply(settings: &Settings) -> Result<(), String> {
    let exe =
        std::env::current_exe().map_err(|err| format!("could not find SolidLauncher ({err})"))?;
    apply_at(&entry_path()?, &exe, settings)
}

/// [`apply`], against a path and a program given to it, so a test need not touch the real one.
pub fn apply_at(path: &Path, exe: &Path, settings: &Settings) -> Result<(), String> {
    if !settings.start_on_login {
        return remove_entry(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {} ({err})", parent.display()))?;
    }
    backend::write(path, exe, settings.start_minimized)
}

/// Turning the option off removes the file. An entry that is already gone is not an error.
fn remove_entry(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("could not turn off start on login ({err})")),
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_at, entry_path};
    use crate::settings::Settings;

    /// A directory of this test's own, removed on the way out.
    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "solid-launcher-autostart-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A stand-in for the installed launcher, which has to exist on disk.
    fn program(dir: &std::path::Path) -> std::path::PathBuf {
        let exe = dir.join(if cfg!(windows) {
            "solid-launcher.exe"
        } else {
            "solid-launcher"
        });
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&exe, b"not really a program");
        exe
    }

    /// What every backend has to do, whatever its entry looks like inside.
    ///
    /// The contents are a backend's own business — a desktop file, a plist, a binary
    /// shortcut — so they are checked in each backend's tests, not here.
    #[test]
    fn the_login_entry_is_written_and_removed_with_the_option() {
        let dir = scratch("apply");
        let path = dir.join("entry");
        // A real file: a Windows shortcut records what it points at, so the backend reads
        // the program rather than taking the path on trust. In use this is
        // `current_exe()`, which is always there.
        let exe = program(&dir);
        let mut settings = Settings::default();

        if let Err(err) = apply_at(&path, &exe, &settings) {
            // A platform that cannot write one yet says so, and leaves nothing behind.
            assert!(err.contains("not supported yet"), "{err}");
            assert!(!path.exists());
            return;
        }
        assert!(path.is_file());
        assert!(std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 0);

        // Writing it again replaces it rather than failing on a file already there.
        settings.start_minimized = false;
        assert!(apply_at(&path, &exe, &settings).is_ok());
        assert!(path.is_file());

        settings.start_on_login = false;
        assert!(apply_at(&path, &exe, &settings).is_ok());
        assert!(!path.exists());
        // Turning it off twice is not an error.
        assert!(apply_at(&path, &exe, &settings).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn this_platform_knows_where_its_login_entry_goes() {
        let Ok(path) = entry_path() else {
            // No home or APPDATA in this environment; nothing to check.
            return;
        };
        assert!(path.is_absolute(), "{}", path.display());
        assert!(path.file_name().is_some());
    }
}
