//! Find a SolidCraft that is already on this machine, however it got there.
//!
//! SolidLauncher used to see only what it had installed itself: a symlink it wrote in
//! `~/.local/bin`. Someone who had installed SolidCraft from `apt`, `dnf`, Flathub or by
//! hand was told it was not installed, and installing through the launcher would have put
//! a second copy beside the first.
//!
//! Each package manager is asked in turn. Reading what they answer is
//! [`super::pkgquery`], which is pure and tested everywhere; this module is only the
//! asking, which needs the machine they describe.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::pkgquery::{parse_dpkg, parse_flatpak, parse_rpm, parse_version_line};
use super::{Installed, Origin};

/// The identifier SolidCraft's Flatpak and `.deb`/`.rpm` packages are published under.
const FLATPAK_ID: &str = "ai.storyteller.solidcraft";
const PACKAGE: &str = "solidcraft";

/// A SolidCraft installed by something other than this launcher, if there is one.
///
/// Asked in the order the system itself would resolve: a system package is what
/// `solidcraft` on the command line runs once `~/.local/bin` has nothing of ours in it.
pub(super) fn system_install() -> Option<Installed> {
    dpkg().or_else(rpm).or_else(flatpak).or_else(on_path)
}

fn dpkg() -> Option<Installed> {
    let output = run(
        "dpkg-query",
        &["-W", "-f=${db:Status-Status} ${Version}", PACKAGE],
    )?;
    let version = parse_dpkg(&output)?;
    binary_at("/usr/bin/solidcraft").map(|binary| Installed {
        version,
        root: PathBuf::from("/usr"),
        binary,
        origin: Origin::Deb,
    })
}

fn rpm() -> Option<Installed> {
    let output = run("rpm", &["-q", "--qf", "%{VERSION}", PACKAGE])?;
    let version = parse_rpm(&output)?;
    binary_at("/usr/bin/solidcraft").map(|binary| Installed {
        version,
        root: PathBuf::from("/usr"),
        binary,
        origin: Origin::Rpm,
    })
}

fn flatpak() -> Option<Installed> {
    let output = run("flatpak", &["info", FLATPAK_ID])?;
    let version = parse_flatpak(&output)?;
    // A Flatpak has no binary on the filesystem to run; `launch` uses `flatpak run`.
    Some(Installed {
        version,
        root: PathBuf::new(),
        binary: PathBuf::new(),
        origin: Origin::Flatpak,
    })
}

/// A `solidcraft` on `PATH` that no package manager claims: a hand-unpacked tarball, or an
/// AppImage someone dropped in `~/.local/bin`.
fn on_path() -> Option<Installed> {
    let output = run("sh", &["-c", "command -v solidcraft"])?;
    let path = PathBuf::from(output.trim());
    if !path.is_file() {
        return None;
    }
    // Ask the program itself, since nothing else here knows the version.
    let version =
        run(&path.to_string_lossy(), &["--version"]).and_then(|out| parse_version_line(&out))?;
    Some(Installed {
        version,
        root: path.parent().map(Path::to_path_buf).unwrap_or_default(),
        binary: path,
        origin: Origin::Unknown,
    })
}

fn binary_at(path: &str) -> Option<PathBuf> {
    let path = PathBuf::from(path);
    path.is_file().then_some(path)
}

/// Run a command and return its standard output, or `None` when it is not there or failed.
///
/// A missing package manager is the normal case, not an error: most machines have one.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}
