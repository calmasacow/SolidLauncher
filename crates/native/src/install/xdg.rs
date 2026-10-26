//! Install the official SolidCraft tarball into an XDG home directory.
//!
//! Linux and the BSDs share this layout. The tree lands in
//! `~/.local/opt/solidcraft-<version>-<os>-<arch>`. Symlinks in `~/.local/bin` point at it,
//! including the pre-0.8 names (`solvecraft`, `solvecraft-cli`) so an existing pin keeps
//! working. Older trees stay until SolidLauncher is asked to remove them. A file in
//! `~/.local/bin` that is not already a symlink is left alone.

use std::fs::{self, File};
use std::io::{self, copy};
use std::path::Path;
use std::process::{Command, Stdio};

use flate2::read::GzDecoder;
use tar::Archive;

use solid_launcher_core::PackageKind;

use super::{found, safe_parts, version_from_dir, Installed, Origin, RemovedCopies, MAX_UNPACKED};

/// `~/.local/opt` holds one directory per version, so older ones can be swept up.
pub(super) const KEEPS_OLD_VERSIONS: bool = true;

/// Unpack a verified package under `home`.
///
/// The official tarball is the only format installed here. It is the one that unpacks into
/// the home directory with no root password, which is how a hand install of SolidCraft is
/// already laid out. An AppImage is a single file with no such tree, so it is refused
/// rather than installed into a shape the rest of this backend cannot find again.
pub(super) fn install_archive(
    archive: &Path,
    home: &Path,
    kind: PackageKind,
    version: &semver::Version,
) -> Result<Installed, String> {
    let installed = match kind {
        // The tarball carries the whole tree: the program, the command-line tool, icons
        // and the file associations. It is the richest install and needs no password.
        PackageKind::TarGz => install_tarball(archive, home)?,
        // One executable file. No password either, and nothing to unpack.
        PackageKind::AppImage => super::appimage::install(archive, home, version)?,
        // Written under /usr by the package manager, after a password prompt. What it
        // leaves behind is found by asking dpkg or rpm, not by looking where we put it.
        PackageKind::Deb | PackageKind::Rpm => {
            super::syspkg::install(archive, kind)?;
            return detect(home).ok_or_else(|| {
                "the package installed, but SolidCraft was not where it was expected afterwards"
                    .to_string()
            });
        }
        other => {
            return Err(format!(
                "not supported yet: installing a {} on this system.",
                other.label()
            ))
        }
    };
    // Remember the format, so an update uses the same one rather than changing it under
    // the user. Failing to write it is not worth undoing a good install over.
    if let Err(err) = write_method(home, kind) {
        eprintln!("SolidLauncher: {err}");
    }
    Ok(installed)
}

/// Records which format this launcher installed, beside the installs themselves.
const METHOD: &str = ".solid-launcher-method";

fn method_path(home: &Path) -> std::path::PathBuf {
    home.join(".local/opt").join(METHOD)
}

fn write_method(home: &Path, kind: PackageKind) -> Result<(), String> {
    let name = match kind {
        PackageKind::TarGz => "tar",
        PackageKind::AppImage => "appimage",
        PackageKind::Deb => "deb",
        PackageKind::Rpm => "rpm",
        other => return Err(format!("no name recorded for a {}", other.label())),
    };
    let path = method_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {} ({err})", parent.display()))?;
    }
    fs::write(&path, format!("{name}\n"))
        .map_err(|err| format!("could not record the install format ({err})"))
}

/// The format an update should use, taken from the copy that is installed.
///
/// `None` when there is nothing installed, or when what is installed is not ours to
/// replace: a Flatpak is updated by Flatpak, and a `solidcraft` of unknown provenance is
/// not something to overwrite on a guess.
pub(super) fn installed_kind(home: &Path) -> Option<PackageKind> {
    match detect(home)?.origin {
        Origin::Deb => Some(PackageKind::Deb),
        Origin::Rpm => Some(PackageKind::Rpm),
        Origin::Flatpak | Origin::Unknown => None,
        Origin::Launcher => Some(marker_kind(home)),
    }
}

/// Which format this launcher last wrote. An install from before the marker existed
/// reads as a tarball, which is what it was: nothing else was supported then.
fn marker_kind(home: &Path) -> PackageKind {
    match fs::read_to_string(method_path(home))
        .unwrap_or_default()
        .trim()
    {
        "appimage" => PackageKind::AppImage,
        "deb" => PackageKind::Deb,
        "rpm" => PackageKind::Rpm,
        _ => PackageKind::TarGz,
    }
}

/// The SolidCraft on this machine, whoever installed it.
///
/// Our own install is looked for first, because `~/.local/bin` comes before `/usr/bin` on
/// a normal `PATH`: when both exist, ours is the one that runs. Failing that, the package
/// managers are asked, so a copy from `apt`, `dnf` or Flathub is found rather than
/// reported as nothing and then installed over.
pub(super) fn detect(home: &Path) -> Option<Installed> {
    launcher_install(home)
        .or_else(|| super::appimage::detect(home))
        .or_else(found::system_install)
}

/// The copy `~/.local/bin/solidcraft` currently points at, when that link is one of ours.
fn launcher_install(home: &Path) -> Option<Installed> {
    let link = home.join(".local/bin/solidcraft");
    let target = fs::read_link(&link).ok()?;
    let absolute = if target.is_absolute() {
        target
    } else {
        link.parent()?.join(target)
    };
    let root = absolute.parent()?.parent()?.to_path_buf();
    let name = root.file_name()?.to_str()?;
    let version = version_from_dir(name)?;
    let binary = root.join("bin/solidcraft");
    binary.is_file().then_some(Installed {
        version,
        root,
        binary,
        origin: Origin::Launcher,
    })
}

/// Unpack `archive` under `home` and point the launchers at it.
pub(super) fn install_tarball(archive: &Path, home: &Path) -> Result<Installed, String> {
    let opt = home.join(".local/opt");
    fs::create_dir_all(&opt)
        .map_err(|err| format!("could not create {} ({err})", opt.display()))?;
    let staging = opt.join(".solid-launcher-staging");
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|err| format!("could not clear a previous install attempt ({err})"))?;
    }
    fs::create_dir_all(&staging).map_err(|err| format!("could not start the install ({err})"))?;
    let top = match extract(archive, &staging) {
        Ok(top) => top,
        Err(err) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(err);
        }
    };
    let extracted = staging.join(&top);
    let final_dir = opt.join(&top);
    if !final_dir.join("bin/solidcraft").is_file() {
        fs::rename(&extracted, &final_dir)
            .map_err(|err| format!("could not move SolidCraft into place ({err})"))?;
    }
    let _ = fs::remove_dir_all(&staging);
    let binary = final_dir.join("bin/solidcraft");
    if !binary.is_file() {
        return Err("the installed tree has no bin/solidcraft".to_string());
    }
    link_commands(home, &final_dir)?;
    copy_share(home, &final_dir)?;
    write_desktop(home, &home.join(".local/bin/solidcraft"))?;
    refresh_caches(home);
    let version = version_from_dir(&top)
        .ok_or_else(|| format!("{top} is not a SolidCraft version folder"))?;
    Ok(Installed {
        version,
        root: final_dir,
        binary,
        origin: Origin::Launcher,
    })
}

pub(super) fn launch(installed: &Installed) -> Result<(), String> {
    // A Flatpak is not a file on disk to run: the runtime starts it by its identifier.
    if installed.origin == Origin::Flatpak {
        return launch_flatpak();
    }
    let binary = &installed.binary;
    if !binary.is_file() {
        return Err("SolidCraft is not installed".to_string());
    }
    let mut command = Command::new("setsid");
    command
        .arg(binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match command.spawn() {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Command::new(binary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("could not open SolidCraft ({err})")),
        Err(err) => Err(format!("could not open SolidCraft ({err})")),
    }
}

fn launch_flatpak() -> Result<(), String> {
    Command::new("flatpak")
        .args(["run", "ai.storyteller.solidcraft"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("could not open the SolidCraft Flatpak ({err})"))
}

fn extract(archive: &Path, staging: &Path) -> Result<String, String> {
    let file = File::open(archive).map_err(|err| format!("could not open the download ({err})"))?;
    let mut tar = Archive::new(GzDecoder::new(file));
    let entries = tar
        .entries()
        .map_err(|err| format!("could not read the archive ({err})"))?;
    let mut top: Option<String> = None;
    let mut unpacked = 0_u64;
    for entry in entries {
        let mut entry = entry.map_err(|err| format!("the archive is damaged ({err})"))?;
        let path = entry
            .path()
            .map_err(|err| format!("the archive has a path SolidLauncher will not use ({err})"))?
            .into_owned();
        let parts = safe_parts(&path)?;
        if parts.is_empty() {
            continue;
        }
        let first = &parts[0];
        match &top {
            None => {
                if version_from_dir(first).is_none() {
                    return Err(format!(
                        "the archive folder {first} is not a SolidCraft build"
                    ));
                }
                top = Some(first.clone());
            }
            Some(expected) if expected != first => {
                return Err("the archive contains more than one top folder".to_string())
            }
            Some(_) => {}
        }
        let dest = staging.join(parts.join("/"));
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&dest)
                .map_err(|err| format!("could not create {} ({err})", dest.display()))?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
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
        let copied = copy(&mut entry, &mut out)
            .map_err(|err| format!("could not unpack {} ({err})", path.display()))?;
        if copied != size {
            return Err(format!("{} ended early", path.display()));
        }
    }
    let top = top.ok_or_else(|| "the archive is empty".to_string())?;
    let root = staging.join(&top);
    for name in ["solidcraft", "solidcraft-cli"] {
        let bin = root.join("bin").join(name);
        if !bin.is_file() || fs::metadata(&bin).map(|meta| meta.len()).unwrap_or(0) == 0 {
            return Err(format!("the archive is missing bin/{name}"));
        }
        set_executable(&bin)?;
    }
    let legacy = root.join("bin/solvecraft-cli");
    if legacy.is_file() {
        set_executable(&legacy)?;
    }
    Ok(top)
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

fn link_commands(home: &Path, root: &Path) -> Result<(), String> {
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin)
        .map_err(|err| format!("could not create {} ({err})", bin.display()))?;
    let pairs = [
        ("solidcraft", root.join("bin/solidcraft")),
        ("solidcraft-cli", root.join("bin/solidcraft-cli")),
        ("solvecraft", root.join("bin/solidcraft")),
        (
            "solvecraft-cli",
            if root.join("bin/solvecraft-cli").is_file() {
                root.join("bin/solvecraft-cli")
            } else {
                root.join("bin/solidcraft-cli")
            },
        ),
    ];
    for (name, target) in pairs {
        if !target.is_file() {
            return Err(format!("missing {}", target.display()));
        }
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
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("could not update {} ({err})", link.display())),
        }
        std::os::unix::fs::symlink(&target, &link)
            .map_err(|err| format!("could not link {} ({err})", link.display()))?;
    }
    Ok(())
}

fn copy_share(home: &Path, root: &Path) -> Result<(), String> {
    copy_dir(
        &root.join("share/icons/hicolor"),
        &home.join(".local/share/icons/hicolor"),
    )?;
    copy_dir(
        &root.join("share/mime/packages"),
        &home.join(".local/share/mime/packages"),
    )?;
    copy_dir(
        &root.join("share/metainfo"),
        &home.join(".local/share/metainfo"),
    )?;
    mirror_legacy_icons(&home.join(".local/share/icons/hicolor"))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_dir() {
        return Ok(());
    }
    let mut pending = vec![from.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|err| format!("could not read {} ({err})", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|err| format!("could not read {} ({err})", dir.display()))?;
            let path = entry.path();
            let rel = path
                .strip_prefix(from)
                .map_err(|_| format!("could not copy {}", path.display()))?;
            let dest = to.join(rel);
            let file_type = entry
                .file_type()
                .map_err(|err| format!("could not read {} ({err})", path.display()))?;
            if file_type.is_dir() {
                fs::create_dir_all(&dest)
                    .map_err(|err| format!("could not create {} ({err})", dest.display()))?;
                pending.push(path);
            } else if file_type.is_file() {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|err| format!("could not create {} ({err})", parent.display()))?;
                }
                fs::copy(&path, &dest)
                    .map_err(|err| format!("could not copy {} ({err})", path.display()))?;
            }
        }
    }
    Ok(())
}

fn mirror_legacy_icons(hicolor: &Path) -> Result<(), String> {
    if !hicolor.is_dir() {
        return Ok(());
    }
    let mut pending = vec![hicolor.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.file_name().and_then(|name| name.to_str())
                == Some("ai.storyteller.solidcraft.png")
            {
                let dest = path.with_file_name("ai.storyteller.solvecraft.png");
                fs::copy(&path, &dest).map_err(|err| format!("could not copy the icon ({err})"))?;
            }
        }
    }
    Ok(())
}

pub(super) fn write_desktop(home: &Path, binary: &Path) -> Result<(), String> {
    let dir = home.join(".local/share/applications");
    fs::create_dir_all(&dir)
        .map_err(|err| format!("could not create {} ({err})", dir.display()))?;
    let body = format!(
        "[Desktop Entry]\n\
Type=Application\n\
Name=SolidCraft\n\
GenericName=3D CAD\n\
Comment=Parametric 3D CAD: sketch, constrain, extrude, and change a parameter to rebuild\n\
Exec={bin} %f\n\
TryExec={bin}\n\
Icon=ai.storyteller.solidcraft\n\
Terminal=false\n\
StartupNotify=true\n\
StartupWMClass=ai.storyteller.solidcraft\n\
Categories=Graphics;3DGraphics;Engineering;\n\
Keywords=cad;3d;parametric;sketch;step;stl;modelling;modeling;\n\
MimeType=application/x-solidcraft;model/step;model/iges;model/stl;model/3mf;\n\
# Releases before 0.8 used this desktop id; launchers keep pinned entries across the rename.\n\
X-Flatpak-RenamedFrom=ai.storyteller.solvecraft.desktop;\n",
        bin = binary.display()
    );
    for name in [
        "ai.storyteller.solidcraft.desktop",
        "ai.storyteller.solvecraft.desktop",
    ] {
        let path = dir.join(name);
        fs::write(&path, &body)
            .map_err(|err| format!("could not write {} ({err})", path.display()))?;
    }
    Ok(())
}

/// Delete older `~/.local/opt/solidcraft-<version>-linux-*` trees.
///
/// The tree `~/.local/bin/solidcraft` points at is kept. Anything that is not a
/// SolidCraft version directory, including a directory with no `bin/solidcraft`,
/// is left alone. If that symlink is missing, nothing is deleted.
pub(super) fn remove_old_copies(home: &Path) -> RemovedCopies {
    let mut removed = Vec::new();
    let mut errors = Vec::new();
    let Some(active) = detect(home).filter(|found| found.origin.is_ours()) else {
        // Either nothing is installed, or what is installed belongs to a package manager.
        // Deleting around either would leave dpkg or rpm describing files that are gone.
        errors.push(
            "SolidCraft was not installed through SolidLauncher, so older copies were left alone."
                .to_string(),
        );
        return RemovedCopies { removed, errors };
    };
    let active_root = fs::canonicalize(&active.root).unwrap_or(active.root);
    let opt = home.join(".local/opt");
    let entries = match fs::read_dir(&opt) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return RemovedCopies { removed, errors };
        }
        Err(err) => {
            errors.push(format!("could not read {} ({err})", opt.display()));
            return RemovedCopies { removed, errors };
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                errors.push(format!("could not read {} ({err})", opt.display()));
                continue;
            }
        };
        let path = entry.path();
        if path.parent() != Some(opt.as_path()) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let name = name.to_string();
        if version_from_dir(&name).is_none() || !path.join("bin/solidcraft").is_file() {
            continue;
        }
        let canon = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if canon == active_root {
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

fn refresh_caches(home: &Path) {
    let applications = home.join(".local/share/applications");
    if applications.is_dir() {
        let _ = Command::new("update-desktop-database")
            .arg(&applications)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let mime = home.join(".local/share/mime");
    if mime.join("packages").is_dir() {
        let _ = Command::new("update-mime-database")
            .arg(&mime)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let icons = home.join(".local/share/icons/hicolor");
    if icons.is_dir() {
        let _ = Command::new("gtk-update-icon-cache")
            .arg("-t")
            .arg(&icons)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::Builder;

    fn fixture(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).expect("create");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, *name, *bytes)
                .expect("append");
        }
        builder.finish().expect("tar");
    }

    #[test]
    fn installs_into_a_home_directory_and_keeps_the_old_names() {
        let home = std::env::temp_dir().join(format!("solid-launcher-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).expect("home");
        let archive = home.join("app.tar.gz");
        let script = b"#!/bin/sh\necho solidcraft-cli 0.0.1\n";
        fixture(
            &archive,
            &[
                ("solidcraft-0.0.1-linux-x86_64/bin/solidcraft", script),
                ("solidcraft-0.0.1-linux-x86_64/bin/solidcraft-cli", script),
                ("solidcraft-0.0.1-linux-x86_64/bin/solvecraft-cli", script),
                ("solidcraft-0.0.1-linux-x86_64/share/applications/ai.storyteller.solidcraft.desktop", b"[Desktop Entry]\n"),
            ],
        );
        let installed = install_tarball(&archive, &home).expect("install");
        assert_eq!(installed.version.to_string(), "0.0.1");
        let link = fs::read_link(home.join(".local/bin/solvecraft")).expect("legacy link");
        assert!(link.ends_with("bin/solidcraft"));
        let again = install_tarball(&archive, &home).expect("second install");
        assert_eq!(again.version, installed.version);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn rejects_a_path_that_climbs_out_or_a_foreign_folder() {
        let err = safe_parts(Path::new("../escaped")).expect_err("parent");
        assert!(err.contains("escapes"), "{err}");
        assert!(safe_parts(Path::new("/etc/passwd")).is_err());

        let dir = std::env::temp_dir().join(format!("solid-launcher-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let archive = dir.join("bad.tar.gz");
        fixture(&archive, &[("not-solidcraft/bin/solidcraft", b"x")]);
        let err = extract(&archive, &dir).expect_err("foreign folder");
        assert!(err.contains("not a SolidCraft build"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn removes_only_older_solidcraft_trees() {
        let home = std::env::temp_dir().join(format!("solid-launcher-old-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".local/bin")).expect("bin");
        let active = home.join(".local/opt/solidcraft-0.0.1-linux-x86_64");
        let old = home.join(".local/opt/solidcraft-0.0.2-linux-aarch64");
        let freebsd = home.join(".local/opt/solidcraft-0.0.3-freebsd-x86_64");
        let empty = home.join(".local/opt/solidcraft-0.0.4-linux-x86_64");
        let foreign = home.join(".local/opt/not-solidcraft");
        for dir in [&active, &old, &freebsd, &foreign] {
            fs::create_dir_all(dir.join("bin")).expect("tree");
            fs::write(dir.join("bin/solidcraft"), b"bin").expect("binary");
        }
        fs::create_dir_all(&empty).expect("empty tree");
        fs::write(
            home.join(".local/opt/solidcraft-9.9.9-linux-x86_64"),
            b"file",
        )
        .expect("file");
        std::os::unix::fs::symlink(
            active.join("bin/solidcraft"),
            home.join(".local/bin/solidcraft"),
        )
        .expect("link");

        let outcome = remove_old_copies(&home);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            outcome.removed,
            vec![
                "solidcraft-0.0.2-linux-aarch64".to_string(),
                "solidcraft-0.0.3-freebsd-x86_64".to_string(),
            ]
        );
        assert!(active.join("bin/solidcraft").is_file());
        assert!(empty.is_dir());
        assert!(foreign.join("bin/solidcraft").is_file());
        assert!(home
            .join(".local/opt/solidcraft-9.9.9-linux-x86_64")
            .is_file());

        let nowhere =
            std::env::temp_dir().join(format!("solid-launcher-none-{}", std::process::id()));
        let _ = fs::remove_dir_all(&nowhere);
        fs::create_dir_all(nowhere.join(".local/opt/solidcraft-0.0.1-linux-x86_64/bin"))
            .expect("only");
        fs::write(
            nowhere.join(".local/opt/solidcraft-0.0.1-linux-x86_64/bin/solidcraft"),
            b"bin",
        )
        .expect("only bin");
        let kept = remove_old_copies(&nowhere);
        assert!(kept.removed.is_empty());
        assert!(!kept.errors.is_empty());
        assert!(nowhere
            .join(".local/opt/solidcraft-0.0.1-linux-x86_64/bin/solidcraft")
            .is_file());
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&nowhere);
    }
}
