//! Install SolidCraft from a `.deb` or an `.rpm`.
//!
//! The one install here that needs a password. A system package is written under `/usr`
//! and recorded in dpkg's or rpm's database, which no ordinary user may do, so the
//! command runs under `pkexec` and the desktop asks for authentication.
//!
//! That is a real cost: every install and every update prompts. It is offered because a
//! system package is what some people want — it is on `PATH` for every user, it carries
//! the file associations, and `apt remove` or `dnf remove` takes it away again. The
//! launcher's other Linux installs stay password-free and remain the default.
//!
//! What is installed afterwards belongs to the package manager, not to us: [`super::found`]
//! detects it, and nothing here deletes or replaces it behind the package manager's back.

use std::path::Path;
use std::process::{Command, Stdio};

use solid_launcher_core::PackageKind;

/// Hand the package to the system installer, asking for authentication first.
pub(super) fn install(archive: &Path, kind: PackageKind) -> Result<(), String> {
    let path = archive
        .to_str()
        .ok_or("the download path is not valid Unicode")?;
    let (program, args) = match kind {
        // `apt install` on a file resolves dependencies, which `dpkg -i` does not.
        PackageKind::Deb => ("apt-get", vec!["install", "-y", path]),
        // `-U` upgrades when an older one is installed and installs when none is.
        PackageKind::Rpm => ("rpm", vec!["-U", "--replacepkgs", path]),
        other => {
            return Err(format!(
                "not a system package: {} cannot be installed this way",
                other.label()
            ))
        }
    };
    authenticate(program, &args)
}

/// Run a command as root through the desktop's authentication prompt.
///
/// `pkexec` is what a graphical program should use: it shows the desktop's own dialog
/// rather than expecting a terminal, as `sudo` would.
fn authenticate(program: &str, args: &[&str]) -> Result<(), String> {
    if which("pkexec").is_none() {
        return Err(
            "installing a system package needs pkexec, which is not on this computer. \
             Choose a different install method in Options, or install the package by hand."
                .to_string(),
        );
    }
    let status = Command::new("pkexec")
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .map_err(|err| format!("could not ask for permission to install ({err})"))?;
    match status.code() {
        Some(0) => Ok(()),
        // pkexec's own code for a prompt the user dismissed or failed.
        Some(126) => Err("the install was cancelled".to_string()),
        Some(127) => Err(format!("{program} is not on this computer")),
        Some(code) => Err(format!("{program} stopped with code {code}")),
        None => Err(format!("{program} did not finish")),
    }
}

fn which(program: &str) -> Option<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {program}"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|path| !path.is_empty())
}
