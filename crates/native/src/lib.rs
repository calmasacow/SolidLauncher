//! Download SolidCraft, check it, install it under the home directory, and open it.
//!
//! [`install`] picks the backend for the platform this was built for; everything else here
//! is the same on all of them.

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

mod http;
mod install;
mod shortcut;

pub use http::{check_status, install_latest, Status};
pub use install::{
    detect, home_dir, install_archive, keeps_old_versions, launch, list, remove, remove_old_copies,
    Installed, Installs, Origin, RemovedCopies,
};
#[cfg(windows)]
pub use shortcut::write_shortcut;
/// Re-exported so the window can name a package without depending on the core crate.
pub use solid_launcher_core::PackageKind;

/// Sent on every request. No name, email or account id.
pub const USER_AGENT: &str = concat!("SolidLauncher/", env!("CARGO_PKG_VERSION"));
