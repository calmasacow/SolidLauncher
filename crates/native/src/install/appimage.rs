//! Install SolidCraft as an AppImage.
//!
//! An AppImage is one executable file, so there is nothing to unpack and nothing to put
//! on a system path: it is copied into `~/.local/opt`, made executable, and pointed at
//! from `~/.local/bin` and a desktop entry. No root, like every other install this
//! launcher does on Linux.
//!
//! The file carries no version where anything can read it, so the name it is saved under
//! does: `solidcraft-<version>-linux-<arch>.AppImage`, which is the name the release
//! publishes it as.

use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;

use super::{version_from_dir, Installed, Origin};

/// Where AppImages are kept, beside the unpacked trees.
pub(super) fn dir(home: &Path) -> PathBuf {
    home.join(".local/opt")
}

/// The AppImage `~/.local/bin/solidcraft` points at, when that link is one of ours.
pub(super) fn detect(home: &Path) -> Option<Installed> {
    let link = home.join(".local/bin/solidcraft");
    let target = fs::read_link(&link).ok()?;
    let absolute = if target.is_absolute() {
        target
    } else {
        link.parent()?.join(target)
    };
    let name = absolute.file_name()?.to_str()?;
    let version = version_from_appimage(name)?;
    absolute.is_file().then_some(Installed {
        version,
        root: absolute.parent().map(Path::to_path_buf).unwrap_or_default(),
        binary: absolute,
        origin: Origin::Launcher,
    })
}

/// `solidcraft-0.22.1-linux-x86_64.AppImage` holds its version in its name.
pub(super) fn version_from_appimage(name: &str) -> Option<Version> {
    version_from_dir(name.strip_suffix(".AppImage")?)
}

/// Copy the verified AppImage into place and point the launchers at it.
pub(super) fn install(archive: &Path, home: &Path, version: &Version) -> Result<Installed, String> {
    let arch = super::target_token();
    let name = format!("solidcraft-{version}-{arch}.AppImage");
    // Refuse a name this backend could not read back, rather than installing something
    // `detect` would then report as missing.
    version_from_appimage(&name)
        .ok_or_else(|| format!("{name} is not a name SolidLauncher can read a version out of"))?;

    let dir = dir(home);
    fs::create_dir_all(&dir)
        .map_err(|err| format!("could not create {} ({err})", dir.display()))?;
    let dest = dir.join(&name);
    // Copy rather than rename: the download sits in the cache, which may be on another
    // filesystem, and a rename across one fails.
    fs::copy(archive, &dest).map_err(|err| format!("could not put SolidCraft in place ({err})"))?;
    set_executable(&dest)?;

    link(home, &dest)?;
    super::xdg::write_desktop(home, &home.join(".local/bin/solidcraft"))?;
    Ok(Installed {
        version: version.clone(),
        root: dir,
        binary: dest,
        origin: Origin::Launcher,
    })
}

fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|err| format!("could not read {} ({err})", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|err| format!("could not mark {} executable ({err})", path.display()))
}

/// `~/.local/bin/solidcraft` and the pre-0.8 `solvecraft` both point at the AppImage.
///
/// There is no separate command-line program inside an AppImage, so `solidcraft-cli` is
/// not linked: a link to the AppImage under that name would open the window.
fn link(home: &Path, target: &Path) -> Result<(), String> {
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin)
        .map_err(|err| format!("could not create {} ({err})", bin.display()))?;
    for name in ["solidcraft", "solvecraft"] {
        let link = bin.join(name);
        match fs::symlink_metadata(&link) {
            Ok(meta) if meta.file_type().is_symlink() => {
                fs::remove_file(&link)
                    .map_err(|err| format!("could not update {} ({err})", link.display()))?;
            }
            Ok(_) => {
                return Err(format!(
                    "{} already exists and is not a symlink, so it was left as it is",
                    link.display()
                ))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("could not update {} ({err})", link.display())),
        }
        std::os::unix::fs::symlink(target, &link)
            .map_err(|err| format!("could not link {} ({err})", link.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::version_from_appimage;

    #[test]
    fn reads_the_version_out_of_an_appimage_name() {
        for (name, want) in [
            ("solidcraft-0.22.1-linux-x86_64.AppImage", "0.22.1"),
            ("solidcraft-1.0.0-linux-aarch64.AppImage", "1.0.0"),
        ] {
            assert_eq!(
                version_from_appimage(name)
                    .map(|v| v.to_string())
                    .as_deref(),
                Some(want),
                "{name}"
            );
        }
        for name in [
            "solidcraft-0.22.1-linux-x86_64.tar.gz",
            "solidcraft-linux-x86_64.AppImage",
            "something-else.AppImage",
            "",
        ] {
            assert_eq!(version_from_appimage(name), None, "{name}");
        }
    }
}
