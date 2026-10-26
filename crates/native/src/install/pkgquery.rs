//! Read a version out of what a package manager prints.
//!
//! Pure, and compiled wherever the suite runs rather than only on the platforms that have
//! these package managers, so the awkward cases are covered without needing dpkg or rpm
//! to be installed. Running the commands is [`super::found`]'s job.

use semver::Version;

/// `dpkg-query -W -f='${db:Status-Status} ${Version}'` prints `installed 0.22.1`.
///
/// A package that was removed but whose configuration remains still has a row, with a
/// status of `config-files`, so the status word has to be read rather than assumed.
pub(super) fn parse_dpkg(output: &str) -> Option<Version> {
    let line = output.lines().next()?.trim();
    let (status, version) = line.split_once(' ')?;
    if status != "installed" {
        return None;
    }
    parse_package_version(version)
}

/// `rpm -q --qf '%{VERSION}'` prints the version alone.
pub(super) fn parse_rpm(output: &str) -> Option<Version> {
    parse_package_version(output.trim())
}

/// `flatpak info <id>` prints a block of `Key: value` lines, one of them `Version`.
pub(super) fn parse_flatpak(output: &str) -> Option<Version> {
    output.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim().eq_ignore_ascii_case("version"))
            .then(|| parse_package_version(value.trim()))
            .flatten()
    })
}

/// `solidcraft --version` prints something like `solidcraft 0.22.1`.
pub(super) fn parse_version_line(output: &str) -> Option<Version> {
    output
        .lines()
        .next()?
        .split_whitespace()
        .find_map(parse_package_version)
}

/// A distribution's version may carry packaging on the end: an epoch (`1:0.22.1`), a
/// release (`0.22.1-2`), or a distribution tag (`0.22.1-1.fc40`). None of that is part of
/// SolidCraft's own version, and keeping it would make every comparison with a release
/// wrong.
fn parse_package_version(raw: &str) -> Option<Version> {
    let without_epoch = raw.split_once(':').map_or(raw, |(_, rest)| rest);
    let without_release = without_epoch
        .split_once('-')
        .map_or(without_epoch, |(head, _)| head);
    solid_launcher_core::parse_version(without_release.trim())
}

#[cfg(test)]
mod tests {
    use super::{parse_dpkg, parse_flatpak, parse_rpm, parse_version_line};

    #[test]
    fn reads_an_installed_debian_package_and_ignores_a_removed_one() {
        assert_eq!(
            parse_dpkg("installed 0.22.1")
                .map(|v| v.to_string())
                .as_deref(),
            Some("0.22.1")
        );
        // Removed but not purged: dpkg still has a row for it.
        assert_eq!(parse_dpkg("config-files 0.22.1"), None);
        assert_eq!(parse_dpkg("not-installed "), None);
        assert_eq!(parse_dpkg(""), None);
    }

    #[test]
    fn strips_the_packaging_from_a_distribution_version() {
        // A Debian revision, an RPM release with a distribution tag, and an epoch.
        for raw in ["0.22.1-2", "0.22.1-1.fc40", "1:0.22.1", "1:0.22.1-3"] {
            assert_eq!(
                parse_rpm(raw).map(|v| v.to_string()).as_deref(),
                Some("0.22.1"),
                "{raw}"
            );
        }
    }

    #[test]
    fn reads_the_version_out_of_a_flatpak_info_block() {
        let output = "\
SolidCraft - Parametric 3D CAD

          ID: ai.storyteller.solidcraft
         Ref: app/ai.storyteller.solidcraft/x86_64/stable
        Arch: x86_64
     Version: 0.22.1
     License: MIT OR Apache-2.0
";
        assert_eq!(
            parse_flatpak(output).map(|v| v.to_string()).as_deref(),
            Some("0.22.1")
        );
        // A block with no Version line is not an error, just nothing.
        assert_eq!(parse_flatpak("ID: ai.storyteller.solidcraft\n"), None);
        assert_eq!(parse_flatpak(""), None);
    }

    #[test]
    fn reads_the_version_a_program_prints() {
        assert_eq!(
            parse_version_line("solidcraft 0.22.1\n")
                .map(|v| v.to_string())
                .as_deref(),
            Some("0.22.1")
        );
        assert_eq!(
            parse_version_line("SolidCraft 1.0.0-rc.2")
                .map(|v| v.to_string())
                .as_deref(),
            Some("1.0.0")
        );
        assert_eq!(parse_version_line("no version here"), None);
        assert_eq!(parse_version_line(""), None);
    }
}
