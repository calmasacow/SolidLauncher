//! Install SolidCraft on macOS.
//!
//! The release publishes a `.dmg` and no tarball, so the app is copied out of the mounted
//! image into `~/Applications` — this user's own folder, so nothing asks for a password.
//! Only one copy exists at a time: the bundle carries its version inside, not in its name,
//! and a second copy would be a second entry in Launchpad rather than a spare to fall back
//! on.
//!
//! ```text
//! ~/Applications/SolidCraft.app/Contents/Info.plist   CFBundleShortVersionString
//! ~/Applications/SolidCraft.app/Contents/MacOS/SolidCraft
//! ```
//!
//! Because the version is read out of the bundle, a copy the user dragged across from the
//! disk image themselves is found too, not only one this launcher installed.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use semver::Version;
use solid_launcher_core::PackageKind;

use super::{plist, Installed, RemovedCopies};

/// The bundle is replaced in place, so there are never older copies to sweep.
pub(super) const KEEPS_OLD_VERSIONS: bool = false;

const APP: &str = "SolidCraft.app";
/// `CFBundleExecutable` in SolidCraft's bundle.
const EXECUTABLE: &str = "SolidCraft";

fn applications(home: &Path) -> PathBuf {
    home.join("Applications")
}

fn installed_app(home: &Path) -> PathBuf {
    applications(home).join(APP)
}

pub(super) fn detect(home: &Path) -> Option<Installed> {
    let app = installed_app(home);
    let version = bundle_version(&app)?;
    let binary = app.join("Contents").join("MacOS").join(EXECUTABLE);
    binary.is_file().then_some(Installed {
        version,
        root: app,
        binary,
    })
}

/// The version an installed bundle reports.
///
/// SolidCraft's `CFBundleShortVersionString` drops any pre-release suffix, so a `0.23.0-rc.1`
/// bundle reads as `0.23.0`. SolidLauncher refuses to install a pre-release in the first
/// place, so the two agree for anything it put there itself.
fn bundle_version(app: &Path) -> Option<Version> {
    let text = std::fs::read_to_string(app.join("Contents").join("Info.plist")).ok()?;
    let value = plist::string_value(&text, "CFBundleShortVersionString")?;
    solid_launcher_core::parse_version(&value)
}

pub(super) fn install_archive(
    archive: &Path,
    home: &Path,
    kind: PackageKind,
    _version: &Version,
) -> Result<Installed, String> {
    if kind != PackageKind::Dmg {
        return Err(format!(
            "not supported yet: installing a {} on macOS. SolidLauncher installs the disk image.",
            kind.label()
        ));
    }
    let applications = applications(home);
    std::fs::create_dir_all(&applications)
        .map_err(|err| format!("could not create {} ({err})", applications.display()))?;

    let mount = mount_point();
    attach(archive, &mount)?;
    // Whatever happens inside, the image is unmounted before returning: a disk image left
    // attached sits in Finder's sidebar until the user ejects it by hand.
    let result = copy_app(&mount, &applications);
    detach(&mount);
    result?;

    let app = installed_app(home);
    unquarantine(&app);
    detect(home).ok_or_else(|| {
        format!(
            "SolidCraft was copied to {}, but its version could not be read afterwards",
            app.display()
        )
    })
}

fn copy_app(mount: &Path, applications: &Path) -> Result<(), String> {
    let source = mount.join(APP);
    if !source.join("Contents").join("Info.plist").is_file() {
        return Err("the disk image does not contain SolidCraft".to_string());
    }
    let destination = applications.join(APP);
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(|err| {
            format!(
                "could not replace {} ({err}). Quit SolidCraft and try again.",
                destination.display()
            )
        })?;
    }
    // `ditto` rather than a recursive copy: it keeps symlinks, permissions and the bundle's
    // own metadata, which a hand-rolled copy loses and the app then will not start.
    let status = Command::new("/usr/bin/ditto")
        .arg(&source)
        .arg(&destination)
        .stdin(Stdio::null())
        .status()
        .map_err(|err| format!("could not copy SolidCraft ({err})"))?;
    if !status.success() {
        return Err("could not copy SolidCraft out of the disk image".to_string());
    }
    Ok(())
}

/// A mount point of our own under the temporary directory, so two runs cannot collide.
fn mount_point() -> PathBuf {
    std::env::temp_dir().join(format!(
        "solid-launcher-dmg-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default()
    ))
}

fn attach(archive: &Path, mount: &Path) -> Result<(), String> {
    std::fs::create_dir_all(mount)
        .map_err(|err| format!("could not prepare the disk image ({err})"))?;
    let status = Command::new("/usr/bin/hdiutil")
        .arg("attach")
        // -nobrowse keeps it out of Finder's sidebar; -readonly means nothing in the image
        // can be changed while it is open.
        .args(["-nobrowse", "-readonly", "-noverify", "-mountpoint"])
        .arg(mount)
        .arg(archive)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(|err| format!("could not open the disk image ({err})"))?;
    if !status.success() {
        let _ = std::fs::remove_dir(mount);
        return Err("could not open the disk image".to_string());
    }
    Ok(())
}

fn detach(mount: &Path) {
    let detached = Command::new("/usr/bin/hdiutil")
        .arg("detach")
        .arg(mount)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !matches!(detached, Ok(status) if status.success()) {
        // Busy right after a copy is common; force is the documented answer.
        let _ = Command::new("/usr/bin/hdiutil")
            .args(["detach", "-force"])
            .arg(mount)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = std::fs::remove_dir(mount);
}

/// Clear the quarantine flag macOS puts on anything downloaded.
///
/// Deliberate. SolidCraft is ad-hoc signed and not notarized, so without this every update
/// needs a right-click and Open before it will start. SolidLauncher has already checked the
/// download against the SHA-256 the release published, which is the guarantee being relied
/// on here — so that check must stay exactly as strict as it is.
fn unquarantine(app: &Path) {
    let cleared = Command::new("/usr/bin/xattr")
        .args(["-d", "-r", "com.apple.quarantine"])
        .arg(app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !matches!(cleared, Ok(status) if status.success()) {
        eprintln!(
            "SolidLauncher: could not clear the quarantine flag on {}. \
             The first start may need right-click then Open.",
            app.display()
        );
    }
}

pub(super) fn launch(binary: &Path) -> Result<(), String> {
    if !binary.is_file() {
        return Err("SolidCraft is not installed".to_string());
    }
    // Open the bundle, not the program inside it: that is what gives it its icon, its name
    // in the Dock and its place in the window server.
    let bundle = bundle_of(binary);
    let status = Command::new("/usr/bin/open")
        .arg(bundle.as_deref().unwrap_or(binary))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| format!("could not open SolidCraft ({err})"))?;
    if !status.success() {
        return Err("could not open SolidCraft".to_string());
    }
    Ok(())
}

/// `…/SolidCraft.app` for `…/SolidCraft.app/Contents/MacOS/SolidCraft`.
fn bundle_of(binary: &Path) -> Option<PathBuf> {
    let bundle = binary.parent()?.parent()?.parent()?;
    (bundle.extension().is_some_and(|ext| ext == "app")).then(|| bundle.to_path_buf())
}

pub(super) fn remove_old_copies(_home: &Path) -> RemovedCopies {
    RemovedCopies {
        removed: Vec::new(),
        errors: vec![
            "macOS keeps one copy of SolidCraft, which each update replaces, so there is \
             nothing to remove."
                .to_string(),
        ],
    }
}
