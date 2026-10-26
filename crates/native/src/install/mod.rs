//! Install SolidCraft under the home directory, and open it.
//!
//! One backend per platform, chosen at compile time. They differ in what they unpack and
//! where it lands; everything above this module sees the same four calls, so `http.rs` and
//! the window never branch on the operating system.
//!
//! | Platform | Backend | Package |
//! |---|---|---|
//! | Linux, the BSDs | [`xdg`] | the official `.tar.gz` into `~/.local` |
//! | Windows | [`windows`] | the portable `.zip`, or the `.msi` |
//! | macOS | [`macos`] | `SolidCraft.app` out of the `.dmg` |
//!
//! Nothing here asks for a password: every backend installs for the current user only.

use std::path::{Path, PathBuf};

use semver::Version;
use solid_launcher_core::{parse_version, PackageKind};

// Windows installs through these. They are also compiled under `cfg(test)` everywhere, so
// the zip-slip guard and the layout rules are covered by the suite wherever it runs rather
// than only on a Windows runner.
// macOS reads the installed version out of the bundle; the reader is covered by the
// suite everywhere for the same reason.
#[cfg(any(target_os = "macos", test))]
mod plist;
#[cfg(any(windows, test))]
mod portable;
#[cfg(any(windows, test))]
mod zip;

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

/// The largest tree any backend will write, before compression is taken into account.
///
/// An archive that claims to unpack to more than this is refused rather than filling the
/// disk. SolidCraft's own packages are a long way under it.
// The XDG backend uses this. The Windows and macOS backends will too once they are
// filled in, so on those targets it is dead for now rather than unwanted.
#[allow(dead_code)]
pub(crate) const MAX_UNPACKED: u64 = 1024 * 1024 * 1024;

/// A SolidCraft tree this launcher can open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub version: Version,
    pub root: PathBuf,
    pub binary: PathBuf,
}

/// Older SolidCraft trees this launcher may delete, and why a tree was skipped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemovedCopies {
    pub removed: Vec<String>,
    pub errors: Vec<String>,
}

pub fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "no home directory".to_string())
}

/// The SolidCraft this launcher installed, if it is still there.
pub fn detect(home: &Path) -> Option<Installed> {
    backend::detect(home)
}

/// Unpack a verified `archive` of `kind` under `home` and point the launchers at it.
///
/// `archive` has already been checked against the SHA-256 the release published. A backend
/// refuses a `kind` it does not install rather than guessing.
///
/// `version` is the one the release carries. A backend that can read the version back off
/// what it installed prefers that, because it describes the files actually on disk; one
/// that cannot, such as an MSI install, records this instead.
pub fn install_archive(
    archive: &Path,
    home: &Path,
    kind: PackageKind,
    version: &Version,
) -> Result<Installed, String> {
    backend::install_archive(archive, home, kind, version)
}

pub fn launch(binary: &Path) -> Result<(), String> {
    backend::launch(binary)
}

/// Delete SolidCraft trees older than the one in use.
pub fn remove_old_copies(home: &Path) -> RemovedCopies {
    backend::remove_old_copies(home)
}

/// Whether this platform keeps several versions side by side.
///
/// Where it does not, the window hides the "Remove old versions" option rather than
/// offering a switch that does nothing.
pub fn keeps_old_versions() -> bool {
    backend::KEEPS_OLD_VERSIONS
}

/// The version in a `solidcraft-<version>-<os>-<arch>` directory name.
///
/// The OS and architecture are whatever the release used, so a build for a platform this
/// launcher learns about later still parses. A name that is not a SolidCraft version
/// directory returns `None`, which is what keeps [`remove_old_copies`] away from folders it
/// did not create.
// The XDG backend uses this. The Windows and macOS backends will too once they are
// filled in, so on those targets it is dead for now rather than unwanted.
#[allow(dead_code)]
pub(crate) fn version_from_dir(name: &str) -> Option<Version> {
    let rest = name.strip_prefix("solidcraft-")?;
    let mut parts = rest.rsplitn(3, '-');
    let (_arch, _os, version) = (parts.next()?, parts.next()?, parts.next()?);
    if !version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    parse_version(version)
}

/// The components of a relative archive path, refusing anything that climbs out of it.
///
/// Archive entries are hostile input: a `..`, an absolute path or a drive prefix would write
/// outside the staging directory. Shared by every backend that unpacks an archive, because
/// zip-slip and tar-slip are the same bug.
// The XDG backend uses this. The Windows and macOS backends will too once they are
// filled in, so on those targets it is dead for now rather than unwanted.
#[allow(dead_code)]
pub(crate) fn safe_parts(path: &Path) -> Result<Vec<String>, String> {
    use std::path::Component;
    if path.is_absolute() {
        return Err("the archive contains an absolute path".to_string());
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_str().ok_or_else(|| {
                    "the archive has a path SolidLauncher will not use".to_string()
                })?;
                if text.is_empty() || text == "." || text == ".." || text.contains('\0') {
                    return Err("the archive path escapes its folder".to_string());
                }
                parts.push(text.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("the archive path escapes its folder".to_string());
            }
        }
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_out_of_any_platforms_directory_name() {
        for (name, want) in [
            ("solidcraft-0.22.1-linux-x86_64", "0.22.1"),
            ("solidcraft-0.22.1-linux-aarch64", "0.22.1"),
            ("solidcraft-0.22.1-freebsd-x86_64", "0.22.1"),
            ("solidcraft-1.0.0-windows-x64", "1.0.0"),
            ("solidcraft-0.9.0-macos-universal", "0.9.0"),
        ] {
            let got = version_from_dir(name);
            assert_eq!(got.map(|v| v.to_string()).as_deref(), Some(want), "{name}");
        }
        for name in [
            "not-solidcraft",
            "solidcraft",
            "solidcraft-linux-x86_64",
            "solidcraft-nope-linux-x86_64",
            "solvecraft-0.22.1-linux-x86_64",
            "",
        ] {
            assert_eq!(version_from_dir(name), None, "{name}");
        }
    }

    #[test]
    fn refuses_an_archive_path_that_climbs_out() {
        assert_eq!(
            safe_parts(Path::new("solidcraft-0.1.0-linux-x86_64/bin/solidcraft")),
            Ok(vec![
                "solidcraft-0.1.0-linux-x86_64".to_string(),
                "bin".to_string(),
                "solidcraft".to_string(),
            ])
        );
        for bad in ["../evil", "a/../../evil", "/etc/passwd"] {
            assert!(safe_parts(Path::new(bad)).is_err(), "{bad}");
        }
    }
}
