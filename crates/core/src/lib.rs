//! SolidCraft release data: parse a GitHub release and choose the build for this computer.
//!
//! Nothing here touches the network or the disk. Downloads must stay on the official release
//! URL, and checksums must look like checksums.

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo,
        clippy::unreachable
    )
)]

mod platform;
mod release;
mod version;

pub use platform::{pick_asset, Arch, Os, PackageKind, Platform};
pub use release::{download_url_ok, parse_github_release, parse_sha256sums, Asset, Release, REPO};
pub use version::parse as parse_version;
