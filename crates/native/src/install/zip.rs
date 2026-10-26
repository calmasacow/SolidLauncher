//! Unpack the portable SolidCraft zip.
//!
//! Compiled on every platform, not only Windows, so the guards below are tested wherever
//! the suite runs. The archive is hostile input even after its SHA-256 matched the release:
//! a zip can carry absolute paths, `..` segments and entries that claim to be small and
//! then stream gigabytes. Each of those is refused rather than clamped.

use std::fs::{self, File};
use std::io::{copy, Read as _};
use std::path::Path;

use super::{safe_parts, MAX_UNPACKED};

/// Unpack `archive` into `staging` and return the name of its single top folder.
///
/// Every entry must sit under one top-level directory, which is how the installed tree is
/// found again afterwards. An archive with two, or with none, is refused.
pub(super) fn extract(archive: &Path, staging: &Path) -> Result<String, String> {
    let file = File::open(archive).map_err(|err| format!("could not open the download ({err})"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|err| format!("could not read the archive ({err})"))?;
    let mut top: Option<String> = None;
    let mut unpacked = 0_u64;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| format!("the archive is damaged ({err})"))?;
        // `enclosed_name` is None for exactly the paths we refuse; `safe_parts` then says
        // which kind it was, so the message tells the user something.
        let path = entry
            .enclosed_name()
            .ok_or_else(|| "the archive path escapes its folder".to_string())?;
        let parts = safe_parts(&path)?;
        if parts.is_empty() {
            continue;
        }
        let first = parts[0].clone();
        match &top {
            None => top = Some(first),
            Some(expected) if expected != &first => {
                return Err("the archive contains more than one top folder".to_string())
            }
            Some(_) => {}
        }
        let dest = staging.join(parts.join("/"));
        if entry.is_dir() {
            fs::create_dir_all(&dest)
                .map_err(|err| format!("could not create {} ({err})", dest.display()))?;
            continue;
        }
        if !entry.is_file() {
            return Err(format!("the archive contains a link ({})", path.display()));
        }
        let size = entry.size();
        unpacked = unpacked
            .checked_add(size)
            .ok_or_else(|| "the archive is too large".to_string())?;
        if unpacked > MAX_UNPACKED {
            return Err("the archive unpacks to more than 1 GB".to_string());
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("could not create {} ({err})", parent.display()))?;
        }
        let mut out = File::create(&dest)
            .map_err(|err| format!("could not write {} ({err})", dest.display()))?;
        // Copy at most what the header promised: a zip bomb lies in that field.
        let copied = copy(&mut entry.by_ref().take(size), &mut out)
            .map_err(|err| format!("could not unpack {} ({err})", path.display()))?;
        if copied != size {
            return Err(format!("{} ended early", path.display()));
        }
    }
    top.ok_or_else(|| "the archive is empty".to_string())
}

#[cfg(test)]
mod tests {
    use super::extract;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("solid-launcher-zip-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn build(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).expect("create");
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("start");
            writer.write_all(bytes).expect("write");
        }
        writer.finish().expect("finish");
    }

    #[test]
    fn unpacks_a_portable_zip_under_its_one_top_folder() {
        let dir = scratch("ok");
        let archive = dir.join("portable.zip");
        build(
            &archive,
            &[
                (
                    "solidcraft-0.22.1-windows-x64-portable/solidcraft.exe",
                    b"MZ",
                ),
                (
                    "solidcraft-0.22.1-windows-x64-portable/solidcraft-cli.exe",
                    b"MZ",
                ),
                ("solidcraft-0.22.1-windows-x64-portable/NOTICE", b"hello"),
            ],
        );
        let staging = dir.join("staging");
        let top = extract(&archive, &staging).expect("extract");
        assert_eq!(top, "solidcraft-0.22.1-windows-x64-portable");
        assert!(staging.join(&top).join("solidcraft.exe").is_file());
        assert_eq!(
            std::fs::read(staging.join(&top).join("NOTICE")).unwrap_or_default(),
            b"hello"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_an_entry_that_climbs_out_of_the_staging_folder() {
        let dir = scratch("escape");
        let archive = dir.join("evil.zip");
        build(&archive, &[("top/../../etc/passwd", b"root")]);
        let staging = dir.join("staging");
        assert!(extract(&archive, &staging).is_err());
        assert!(!dir.join("etc").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_an_archive_with_two_top_folders_or_none() {
        let dir = scratch("tops");
        let two = dir.join("two.zip");
        build(&two, &[("a/x", b"1"), ("b/y", b"2")]);
        assert!(extract(&two, &dir.join("s1")).is_err());

        let empty = dir.join("empty.zip");
        build(&empty, &[]);
        assert!(extract(&empty, &dir.join("s2")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
