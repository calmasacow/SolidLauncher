//! Choose the SolidCraft build that runs on this computer.
//!
//! Release files are named `solidcraft-<version>-<os>-<arch>.<ext>`, the same pattern ArtCraft
//! Launcher uses. Checksums, zsync sidecars, the CLI zip and the web bundle are not the app.

use crate::release::{Asset, Release};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
    FreeBsd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X64,
    X86,
    Arm64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageKind {
    Msi,
    PortableZip,
    Dmg,
    AppImage,
    Deb,
    Rpm,
    Flatpak,
    TarGz,
}

impl PackageKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Msi => "Windows installer",
            Self::PortableZip => "portable zip",
            Self::Dmg => "macOS disk image",
            Self::AppImage => "AppImage",
            Self::Deb => "Debian package",
            Self::Rpm => "RPM package",
            Self::Flatpak => "Flatpak",
            Self::TarGz => "tarball",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Msi => ".msi",
            Self::PortableZip => "-portable.zip",
            Self::Dmg => ".dmg",
            Self::AppImage => ".AppImage",
            Self::Deb => ".deb",
            Self::Rpm => ".rpm",
            Self::Flatpak => ".flatpak",
            Self::TarGz => ".tar.gz",
        }
    }
}

/// The computer SolidLauncher is running on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

impl Platform {
    pub fn current() -> Option<Self> {
        let os = if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(target_os = "freebsd") {
            Os::FreeBsd
        } else {
            return None;
        };
        let arch = if cfg!(target_arch = "x86_64") {
            Arch::X64
        } else if cfg!(target_arch = "x86") {
            Arch::X86
        } else if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else {
            return None;
        };
        Some(Self { os, arch })
    }

    /// The `<os>-<arch>` the release names this machine's build with.
    ///
    /// `None` for a platform SolidCraft publishes nothing for.
    pub fn token(self) -> Option<&'static str> {
        self.tokens().first().copied()
    }

    fn tokens(self) -> &'static [&'static str] {
        match (self.os, self.arch) {
            (Os::Windows, Arch::X64) => &["windows-x64", "windows-x86"],
            (Os::Windows, Arch::X86) => &["windows-x86"],
            (Os::Windows, Arch::Arm64) => &["windows-arm64", "windows-x64"],
            (Os::MacOs, Arch::X64 | Arch::Arm64) => &["macos-universal"],
            (Os::MacOs, Arch::X86) => &[],
            (Os::Linux, Arch::X64) => &["linux-x86_64"],
            (Os::Linux, Arch::Arm64) => &["linux-aarch64"],
            (Os::Linux, Arch::X86) => &[],
            (Os::FreeBsd, Arch::X64) => &["freebsd-x86_64"],
            (Os::FreeBsd, _) => &[],
        }
    }

    /// Formats this launcher installs, best first.
    ///
    /// Linux uses the official tarball. It unpacks into the home directory with no root
    /// password, which is how a hand install of SolidCraft is already laid out. An AppImage
    /// is the fallback when a release has no tarball.
    pub fn install_kinds(self) -> &'static [PackageKind] {
        match self.os {
            Os::Windows => &[PackageKind::Msi, PackageKind::PortableZip],
            Os::MacOs => &[PackageKind::Dmg],
            Os::Linux => &[PackageKind::TarGz, PackageKind::AppImage],
            Os::FreeBsd => &[PackageKind::TarGz],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetName {
    pub version: String,
    pub token: String,
    pub kind: PackageKind,
}

const STEMS: &[&str] = &["solidcraft", "solvecraft"];

/// `<stem>-<version>-<os>-<arch>.<ext>`. Unversioned aliases, `.zsync`, `solidcraft-cli` and
/// `solidcraft-web` do not parse.
pub fn parse_asset_name(name: &str) -> Option<AssetName> {
    const KINDS: [PackageKind; 8] = [
        PackageKind::PortableZip,
        PackageKind::TarGz,
        PackageKind::Msi,
        PackageKind::Dmg,
        PackageKind::AppImage,
        PackageKind::Deb,
        PackageKind::Rpm,
        PackageKind::Flatpak,
    ];
    let (kind, base) = KINDS
        .iter()
        .find_map(|kind| name.strip_suffix(kind.suffix()).map(|base| (*kind, base)))?;
    for stem in STEMS {
        let Some(rest) = base
            .strip_prefix(stem)
            .and_then(|rest| rest.strip_prefix('-'))
        else {
            continue;
        };
        let mut parts = rest.rsplitn(3, '-');
        let (arch, os, version) = (parts.next()?, parts.next()?, parts.next()?);
        if crate::version::parse(version).is_none()
            || !version.starts_with(|c: char| c.is_ascii_digit())
        {
            continue;
        }
        return Some(AssetName {
            version: version.to_string(),
            token: format!("{os}-{arch}"),
            kind,
        });
    }
    None
}

/// The best asset of `release` for `platform`, trying `kinds` in order.
pub fn pick_asset<'a>(
    release: &'a Release,
    platform: Platform,
    kinds: &[PackageKind],
) -> Option<(&'a Asset, PackageKind)> {
    let parsed: Vec<(&Asset, AssetName)> = release
        .assets
        .iter()
        .filter_map(|asset| parse_asset_name(&asset.name).map(|name| (asset, name)))
        .collect();
    for kind in kinds {
        for token in platform.tokens() {
            if let Some((asset, _)) = parsed
                .iter()
                .find(|(_, name)| name.kind == *kind && name.token == *token)
            {
                return Some((asset, *kind));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::parse_github_release;

    fn release() -> Release {
        parse_github_release(include_str!("../tests/fixtures/solidcraft-release.json"))
            .expect("fixture")
    }

    #[test]
    fn linux_x64_prefers_the_versioned_tarball() {
        let release = release();
        let (asset, kind) = pick_asset(
            &release,
            Platform {
                os: Os::Linux,
                arch: Arch::X64,
            },
            Platform {
                os: Os::Linux,
                arch: Arch::X64,
            }
            .install_kinds(),
        )
        .expect("linux tarball");
        assert_eq!(asset.name, "solidcraft-0.22.1-linux-x86_64.tar.gz");
        assert_eq!(kind, PackageKind::TarGz);
    }

    #[test]
    fn other_platforms_pick_their_own_installer() {
        let release = release();
        let name = |os, arch| {
            pick_asset(
                &release,
                Platform { os, arch },
                Platform { os, arch }.install_kinds(),
            )
            .map(|(asset, _)| asset.name.clone())
        };
        assert_eq!(
            name(Os::MacOs, Arch::Arm64).as_deref(),
            Some("solidcraft-0.22.1-macos-universal.dmg")
        );
        assert_eq!(
            name(Os::Windows, Arch::X64).as_deref(),
            Some("solidcraft-0.22.1-windows-x64.msi")
        );
        assert_eq!(
            name(Os::Windows, Arch::Arm64).as_deref(),
            Some("solidcraft-0.22.1-windows-arm64.msi")
        );
        assert_eq!(
            name(Os::Linux, Arch::Arm64).as_deref(),
            Some("solidcraft-0.22.1-linux-aarch64.tar.gz")
        );
        assert_eq!(
            name(Os::FreeBsd, Arch::X64).as_deref(),
            Some("solidcraft-0.22.1-freebsd-x86_64.tar.gz")
        );
    }

    #[test]
    fn ignores_sidecars_cli_web_and_unversioned_aliases() {
        for name in [
            "SHA256SUMS.txt",
            "solidcraft-0.22.1-linux-x86_64.AppImage.zsync",
            "solvecraft-0.22.1-linux-x86_64.AppImage.zsync",
            "solidcraft-cli-0.22.1-macos-universal.zip",
            "solidcraft-web-0.22.1.zip",
            "solidcraft-linux-x86_64.AppImage",
            "SolidCraft-0.22.1-windows-x64.exe",
        ] {
            assert_eq!(parse_asset_name(name), None, "{name}");
        }
    }
}
