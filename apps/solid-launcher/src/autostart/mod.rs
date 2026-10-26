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

#[cfg(target_os = "macos")]
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

    #[test]
    fn the_login_entry_follows_the_two_start_options() {
        let dir = scratch("apply");
        let path = dir.join("entry");
        let exe = std::path::Path::new("/home/someone/.local/bin/solid-launcher");
        let mut settings = Settings::default();

        if let Err(err) = apply_at(&path, exe, &settings) {
            // This platform cannot write a login entry yet; it must not have left one.
            assert!(err.contains("not supported yet"), "{err}");
            assert!(!path.exists());
            return;
        }
        let minimized = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!minimized.is_empty());
        assert!(minimized.contains("--hidden"), "{minimized}");

        settings.start_minimized = false;
        assert!(apply_at(&path, exe, &settings).is_ok());
        let shown = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!shown.contains("--hidden"), "{shown}");
        assert!(shown.contains("solid-launcher"), "{shown}");

        settings.start_on_login = false;
        assert!(apply_at(&path, exe, &settings).is_ok());
        assert!(!path.exists());
        // Turning it off twice is not an error.
        assert!(apply_at(&path, exe, &settings).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_entry_survives_a_program_whose_path_has_a_space() {
        let dir = scratch("spaces");
        let path = dir.join("entry");
        let spaced = std::path::Path::new("/home/some one/bin/solid-launcher");
        let settings = Settings::default();
        match apply_at(&path, spaced, &settings) {
            // A backend that writes an entry has to keep the whole path in it.
            Ok(()) => {
                let body = std::fs::read(&path).unwrap_or_default();
                let text = String::from_utf8_lossy(&body);
                assert!(text.contains("some one"), "{text}");
            }
            // A backend that cannot write one yet says so, and writes nothing.
            Err(err) => {
                assert!(err.contains("not supported yet"), "{err}");
                assert!(!path.exists());
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn this_platform_knows_where_its_login_entry_goes() {
        let path = entry_path().unwrap_or_default();
        assert!(path.is_absolute(), "{}", path.display());
        assert!(path.file_name().is_some());
    }
}
