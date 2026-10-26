//! Install SolidCraft on macOS.
//!
//! The release publishes a `.dmg` and no tarball, so the app is copied out of the mounted
//! image into `~/Applications`. Only one copy exists at a time: the bundle carries its
//! version in `Contents/Info.plist` rather than in its directory name.
//!
//! Not implemented yet: every call reports that rather than half-installing something.

use std::path::Path;

use solid_launcher_core::PackageKind;

use super::{Installed, RemovedCopies};

/// `~/Applications/SolidCraft.app` is replaced in place, so there is nothing to sweep up.
pub(super) const KEEPS_OLD_VERSIONS: bool = false;

const TODO: &str = "not supported yet: SolidLauncher does not install SolidCraft on macOS. \
Download it from https://github.com/tangency-labs/solidcraft/releases/latest";

pub(super) fn detect(_home: &Path) -> Option<Installed> {
    None
}

pub(super) fn install_archive(
    _archive: &Path,
    _home: &Path,
    _kind: PackageKind,
    _version: &semver::Version,
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
