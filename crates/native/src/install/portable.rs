//! The portable layout: one directory per version, under a root this user owns.
//!
//! Used by the Windows backend, but compiled and tested everywhere, because none of it
//! needs a Windows API. What it needs is a root directory, which the backend works out.
//!
//! ```text
//! <root>/current                              the directory name in use
//! <root>/solidcraft-0.22.1-windows-x64/       solidcraft.exe, solidcraft-cli.exe, ...
//! ```
//!
//! `current` stands in for the symlink the XDG layout uses: Windows has no symlink an
//! ordinary user may create. It is read back before it is trusted, so a stale or hand-edited
//! marker reports nothing installed rather than a path that is not there.

use std::fs;
use std::path::Path;

use super::{version_from_dir, zip, Installed, Origin, RemovedCopies};

/// The program the window opens, and the file that proves a directory holds a real build.
pub(super) const EXE: &str = "solidcraft.exe";
/// Names the directory in use. One line, no path: a marker, not a redirect.
const CURRENT: &str = "current";
/// The packaged zip's top folder carries this; the installed directory does not, so that
/// [`version_from_dir`] can read it.
const PORTABLE_SUFFIX: &str = "-portable";

/// The SolidCraft `current` points at, when it is really there.
pub(super) fn detect(root: &Path) -> Option<Installed> {
    let name = fs::read_to_string(root.join(CURRENT)).ok()?;
    let name = name.trim();
    // A marker is a plain directory name. Anything with a separator is not ours.
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return None;
    }
    let version = version_from_dir(name)?;
    let dir = root.join(name);
    let binary = dir.join(EXE);
    binary.is_file().then_some(Installed {
        version,
        root: dir,
        binary,
        origin: Origin::Launcher,
    })
}

/// Unpack the portable zip under `root` and point `current` at it.
pub(super) fn install(archive: &Path, root: &Path) -> Result<Installed, String> {
    fs::create_dir_all(root)
        .map_err(|err| format!("could not create {} ({err})", root.display()))?;
    let staging = root.join(".solid-launcher-staging");
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|err| format!("could not clear a previous install attempt ({err})"))?;
    }
    fs::create_dir_all(&staging).map_err(|err| format!("could not start the install ({err})"))?;

    let top = match zip::extract(archive, &staging) {
        Ok(top) => top,
        Err(err) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(err);
        }
    };
    let result = place(&staging, &top, root);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn place(staging: &Path, top: &str, root: &Path) -> Result<Installed, String> {
    let name = top.strip_suffix(PORTABLE_SUFFIX).unwrap_or(top).to_string();
    let version = version_from_dir(&name)
        .ok_or_else(|| format!("the archive folder {top} is not a SolidCraft build"))?;
    let extracted = staging.join(top);
    let exe = extracted.join(EXE);
    if !exe.is_file() || fs::metadata(&exe).map(|meta| meta.len()).unwrap_or(0) == 0 {
        return Err(format!("the archive is missing {EXE}"));
    }
    let dir = root.join(&name);
    // Reinstalling the version already there keeps it, as the XDG backend does.
    if !dir.join(EXE).is_file() {
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .map_err(|err| format!("could not replace {} ({err})", dir.display()))?;
        }
        fs::rename(&extracted, &dir)
            .map_err(|err| format!("could not move SolidCraft into place ({err})"))?;
    }
    let binary = dir.join(EXE);
    if !binary.is_file() {
        return Err(format!("the installed folder has no {EXE}"));
    }
    fs::write(root.join(CURRENT), format!("{name}\n"))
        .map_err(|err| format!("could not record the installed version ({err})"))?;
    Ok(Installed {
        version,
        root: dir,
        binary,
        origin: Origin::Launcher,
    })
}

/// Delete version directories other than the one `current` points at.
///
/// Nothing is deleted when `current` names nothing real: a launcher that cannot tell which
/// copy is in use must not pick one to keep.
pub(super) fn remove_old(root: &Path) -> RemovedCopies {
    let mut removed = Vec::new();
    let mut errors = Vec::new();
    let Some(active) = detect(root) else {
        errors.push(
            "SolidCraft is not installed through SolidLauncher, so older copies were left alone."
                .to_string(),
        );
        return RemovedCopies { removed, errors };
    };
    let keep = fs::canonicalize(&active.root).unwrap_or_else(|_| active.root.clone());
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return RemovedCopies { removed, errors }
        }
        Err(err) => {
            errors.push(format!("could not read {} ({err})", root.display()));
            return RemovedCopies { removed, errors };
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let name = name.to_string();
        // Only our own version directories, and only ones that really hold a build.
        if version_from_dir(&name).is_none() || !path.join(EXE).is_file() {
            continue;
        }
        if fs::canonicalize(&path).unwrap_or_else(|_| path.clone()) == keep {
            continue;
        }
        match fs::remove_dir_all(&path) {
            Ok(()) => removed.push(name),
            Err(err) => errors.push(format!("could not remove {name} ({err})")),
        }
    }
    removed.sort();
    RemovedCopies { removed, errors }
}

#[cfg(test)]
mod tests {
    use super::{detect, install, remove_old, EXE};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "solid-launcher-portable-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// A stand-in for the release's portable zip.
    fn portable_zip(path: &Path, version: &str) {
        let file = std::fs::File::create(path).expect("create");
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        let top = format!("solidcraft-{version}-windows-x64-portable");
        for name in [EXE, "solidcraft-cli.exe", "NOTICE"] {
            writer
                .start_file(format!("{top}/{name}"), options)
                .expect("start");
            writer.write_all(b"MZ-not-really").expect("write");
        }
        writer.finish().expect("finish");
    }

    #[test]
    fn installs_a_portable_zip_and_finds_it_again() {
        let dir = scratch("install");
        let root = dir.join("SolidCraft");
        let archive = dir.join("portable.zip");
        portable_zip(&archive, "0.22.1");

        let installed = install(&archive, &root).expect("install");
        assert_eq!(installed.version.to_string(), "0.22.1");
        // The directory drops `-portable`, so the version can be read back out of it.
        assert_eq!(
            installed.root.file_name().and_then(|n| n.to_str()),
            Some("solidcraft-0.22.1-windows-x64")
        );
        assert!(installed.binary.is_file());
        assert!(root.join("solidcraft-0.22.1-windows-x64/NOTICE").is_file());
        // Nothing of the staging directory is left behind.
        assert!(!root.join(".solid-launcher-staging").exists());

        let found = detect(&root).expect("detect");
        assert_eq!(found, installed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upgrading_leaves_the_old_copy_until_it_is_swept() {
        let dir = scratch("upgrade");
        let root = dir.join("SolidCraft");
        let old = dir.join("old.zip");
        let new = dir.join("new.zip");
        portable_zip(&old, "0.22.1");
        portable_zip(&new, "0.23.0");

        assert!(install(&old, &root).is_ok());
        let second = install(&new, &root).expect("upgrade");
        assert_eq!(second.version.to_string(), "0.23.0");
        assert_eq!(
            detect(&root).map(|found| found.version.to_string()),
            Some("0.23.0".to_string())
        );
        // Both are on disk until the sweep runs.
        assert!(root.join("solidcraft-0.22.1-windows-x64").is_dir());

        let swept = remove_old(&root);
        assert_eq!(swept.removed, vec!["solidcraft-0.22.1-windows-x64"]);
        assert!(swept.errors.is_empty(), "{:?}", swept.errors);
        assert!(!root.join("solidcraft-0.22.1-windows-x64").exists());
        // The one in use survives.
        assert!(root
            .join("solidcraft-0.23.0-windows-x64")
            .join(EXE)
            .is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_marker_that_points_nowhere_reports_nothing_and_sweeps_nothing() {
        let dir = scratch("marker");
        let root = dir.join("SolidCraft");
        let archive = dir.join("portable.zip");
        portable_zip(&archive, "0.22.1");
        assert!(install(&archive, &root).is_ok());

        for bad in [
            "../../etc",
            "..\\..\\windows",
            ".hidden",
            "solidcraft-0.22.1-windows-x64/..",
            "not-a-version",
            "",
        ] {
            std::fs::write(root.join("current"), bad).expect("marker");
            assert!(detect(&root).is_none(), "{bad}");
            let swept = remove_old(&root);
            assert!(swept.removed.is_empty(), "{bad}: {:?}", swept.removed);
            // The real tree is untouched by any of it.
            assert!(root
                .join("solidcraft-0.22.1-windows-x64")
                .join(EXE)
                .is_file());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_an_archive_that_is_not_a_solidcraft_build() {
        let dir = scratch("foreign");
        let root = dir.join("SolidCraft");
        let archive = dir.join("foreign.zip");
        let file = std::fs::File::create(&archive).expect("create");
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer
            .start_file("something-else/run.exe", options)
            .expect("start");
        writer.write_all(b"MZ").expect("write");
        writer.finish().expect("finish");

        assert!(install(&archive, &root).is_err());
        assert!(detect(&root).is_none());
        assert!(!root.join("something-else").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
