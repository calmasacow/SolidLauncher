//! Install SolidCraft on Windows.
//!
//! Two packages, because they are different installs and either may already be on the
//! machine. Which one was used last is recorded in a `method` file, so [`detect`] knows
//! where to look without guessing.
//!
//! | Method | Where it goes | Elevation |
//! |---|---|---|
//! | portable `.zip` | `%LOCALAPPDATA%\Programs\SolidCraft\<version>` | none |
//! | `.msi` | `%ProgramFiles%\SolidCraft`, machine-wide | a UAC prompt |
//!
//! The portable zip is what the rest of SolidLauncher is built around: it installs for one
//! user with no password, like every other platform here. The MSI is offered because it
//! registers file associations and an entry in Add or remove programs, which the portable
//! copy cannot.
//!
//! The layout work is in [`super::portable`], which is compiled and tested on every
//! platform. What is left here is the part that needs Windows: a Start Menu shortcut,
//! `msiexec`, and starting the program.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use semver::Version;
use solid_launcher_core::PackageKind;

use super::{portable, Installed, Origin, RemovedCopies};

/// The portable layout keeps one directory per version.
pub(super) const KEEPS_OLD_VERSIONS: bool = true;

/// Records which of the two installs was used last: `portable` or `msi <version> <path>`.
const METHOD: &str = "method";

/// Where the portable copies live.
///
/// `LOCALAPPDATA` is followed when the session sets it, because a redirected profile puts
/// it somewhere other than under the home directory.
fn root(home: &Path) -> PathBuf {
    match std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        Some(local) => PathBuf::from(local).join("Programs").join("SolidCraft"),
        None => home
            .join("AppData")
            .join("Local")
            .join("Programs")
            .join("SolidCraft"),
    }
}

pub(super) fn detect(home: &Path) -> Option<Installed> {
    let root = root(home);
    match read_method(&root) {
        Some(Method::Msi { version, exe }) => exe.is_file().then_some(Installed {
            version,
            root: exe.parent().map(Path::to_path_buf).unwrap_or_default(),
            binary: exe,
            origin: Origin::Launcher,
        }),
        // No marker at all: a portable install from before this file existed still has its
        // own, so fall through to it rather than reporting nothing.
        Some(Method::Portable) | None => portable::detect(&root),
    }
}

pub(super) fn install_archive(
    archive: &Path,
    home: &Path,
    kind: PackageKind,
    version: &Version,
) -> Result<Installed, String> {
    let root = root(home);
    match kind {
        PackageKind::PortableZip => {
            let installed = portable::install(archive, &root)?;
            write_method(&root, "portable")?;
            // A Start Menu entry is also what gives notifications an application to come
            // from, so a failure here is worth reporting even though the install worked.
            if let Err(err) = start_menu_shortcut(&installed.binary) {
                eprintln!("SolidLauncher: could not add SolidCraft to the Start menu ({err})");
            }
            Ok(installed)
        }
        PackageKind::Msi => install_msi(archive, &root, version),
        other => Err(format!(
            "not supported yet: installing a {} on Windows. SolidLauncher installs the \
             portable zip or the installer.",
            other.label()
        )),
    }
}

/// Hand the MSI to Windows Installer and record where it put SolidCraft.
///
/// `/qb` shows a progress bar and no questions. The UAC prompt is Windows', not ours: a
/// machine-wide install cannot avoid it, which is why the portable zip is the default.
fn install_msi(archive: &Path, root: &Path, version: &Version) -> Result<Installed, String> {
    let path = archive
        .to_str()
        .ok_or("the download path is not valid Unicode")?;
    let status = Command::new("msiexec")
        .args(["/i", path, "/qb"])
        .stdin(Stdio::null())
        .status()
        .map_err(|err| format!("could not start Windows Installer ({err})"))?;
    match status.code() {
        Some(0) => {}
        // 1602 is the documented code for a user cancelling, including at the UAC prompt.
        Some(1602) => return Err("the install was cancelled".to_string()),
        Some(code) => return Err(format!("Windows Installer stopped with code {code}")),
        None => return Err("Windows Installer did not finish".to_string()),
    }
    let exe = msi_exe()
        .ok_or("the installer finished, but SolidCraft was not where it was expected afterwards")?;
    std::fs::create_dir_all(root)
        .map_err(|err| format!("could not create {} ({err})", root.display()))?;
    write_method(root, &format!("msi {version} {}", exe.display()))?;
    Ok(Installed {
        version: version.clone(),
        root: exe.parent().map(Path::to_path_buf).unwrap_or_default(),
        binary: exe,
        origin: Origin::Launcher,
    })
}

/// Where the MSI puts `solidcraft.exe`.
///
/// `HKLM\Software\SolidCraft\InstallDir` is what the package writes, and it is right even
/// when the install folder was changed. The default is used when that key cannot be read.
fn msi_exe() -> Option<PathBuf> {
    let from_registry = registry_install_dir().map(|dir| dir.join(portable::EXE));
    if let Some(path) = from_registry.filter(|path| path.is_file()) {
        return Some(path);
    }
    let program_files = std::env::var_os("ProgramFiles").filter(|value| !value.is_empty())?;
    let fallback = PathBuf::from(program_files)
        .join("SolidCraft")
        .join(portable::EXE);
    fallback.is_file().then_some(fallback)
}

/// Read `InstallDir` with `reg`, which ships with Windows, rather than taking a crate just
/// to read one string.
fn registry_install_dir() -> Option<PathBuf> {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\Software\SolidCraft",
            "/v",
            "InstallDir",
            "/reg:64",
        ])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    // `    InstallDir    REG_SZ    C:\Program Files\SolidCraft`
    let line = text.lines().find(|line| line.contains("InstallDir"))?;
    let value = line.split("REG_SZ").nth(1)?.trim();
    (!value.is_empty()).then(|| PathBuf::from(value))
}

/// A shortcut in this user's Start menu, replaced on every install so it follows the
/// version in use.
fn start_menu_shortcut(binary: &Path) -> Result<(), String> {
    let appdata = std::env::var_os("APPDATA")
        .filter(|value| !value.is_empty())
        .ok_or("no APPDATA directory")?;
    let dir = PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs");
    crate::shortcut::write_shortcut(binary, &dir.join("SolidCraft.lnk"), None)
}

pub(super) fn launch(installed: &Installed) -> Result<(), String> {
    let binary = &installed.binary;
    if !binary.is_file() {
        return Err("SolidCraft is not installed".to_string());
    }
    Command::new(binary)
        // Start it in its own folder, so it finds solidcraft-cli.exe beside it.
        .current_dir(binary.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("could not open SolidCraft ({err})"))
}

pub(super) fn remove_old_copies(home: &Path) -> RemovedCopies {
    let root = root(home);
    if let Some(Method::Msi { .. }) = read_method(&root) {
        return RemovedCopies {
            removed: Vec::new(),
            errors: vec![
                "SolidCraft was installed with the Windows installer, which replaces the \
                 previous version itself, so there is nothing to remove."
                    .to_string(),
            ],
        };
    }
    portable::remove_old(&root)
}

enum Method {
    Portable,
    Msi { version: Version, exe: PathBuf },
}

fn read_method(root: &Path) -> Option<Method> {
    let text = std::fs::read_to_string(root.join(METHOD)).ok()?;
    let line = text.lines().next()?.trim();
    if line == "portable" {
        return Some(Method::Portable);
    }
    // `msi <version> <path, which may contain spaces>`
    let rest = line.strip_prefix("msi ")?;
    let (version, path) = rest.split_once(' ')?;
    Some(Method::Msi {
        version: solid_launcher_core::parse_version(version)?,
        exe: PathBuf::from(path.trim()),
    })
}

fn write_method(root: &Path, value: &str) -> Result<(), String> {
    std::fs::write(root.join(METHOD), format!("{value}\n"))
        .map_err(|err| format!("could not record how SolidCraft was installed ({err})"))
}
