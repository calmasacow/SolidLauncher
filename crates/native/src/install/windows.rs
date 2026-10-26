//! Install SolidCraft on Windows.
//!
//! Two packages, because they are different installs and a machine may already carry
//! either. The portable `.zip` unpacks into `%LOCALAPPDATA%\Programs\SolidCraft\<version>`
//! with no elevation, which keeps the promise the other backends make. The `.msi` installs
//! for the whole machine through `msiexec` and prompts for administrator rights.
//!
//! Not implemented yet: every call reports that rather than half-installing something.

use std::path::Path;

use solid_launcher_core::PackageKind;

use super::{Installed, RemovedCopies};

/// `%LOCALAPPDATA%\Programs\SolidCraft` holds one directory per version.
pub(super) const KEEPS_OLD_VERSIONS: bool = true;

const TODO: &str = "not supported yet: SolidLauncher does not install SolidCraft on Windows. \
Download it from https://github.com/tangency-labs/solidcraft/releases/latest";

pub(super) fn detect(_home: &Path) -> Option<Installed> {
    None
}

pub(super) fn install_archive(
    _archive: &Path,
    _home: &Path,
    _kind: PackageKind,
) -> Result<Installed, String> {
    Err(TODO.to_string())
}

pub(super) fn launch(_binary: &Path) -> Result<(), String> {
    Err(TODO.to_string())
}

pub(super) fn remove_old_copies(_home: &Path) -> RemovedCopies {
    RemovedCopies {
        removed: Vec::new(),
        errors: vec![TODO.to_string()],
    }
}
