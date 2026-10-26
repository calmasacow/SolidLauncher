//! Write a Windows shortcut.
//!
//! Platform glue, so it lives here rather than in the window: the `.lnk` format is written
//! as bytes, with no COM and nothing for this crate to vouch for.

#[cfg(windows)]
use std::path::Path;

/// Create or replace a `.lnk` at `link` pointing at `target`.
///
/// `arguments` is the command line the shortcut passes, if any.
#[cfg(windows)]
pub fn write_shortcut(target: &Path, link: &Path, arguments: Option<&str>) -> Result<(), String> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {} ({err})", parent.display()))?;
    }
    let mut shortcut = mslnk::ShellLink::new(target)
        .map_err(|err| format!("could not describe the shortcut ({err})"))?;
    if let Some(arguments) = arguments {
        shortcut.set_arguments(Some(arguments.to_string()));
    }
    shortcut
        .create_lnk(link)
        .map_err(|err| format!("could not write {} ({err})", link.display()))
}
