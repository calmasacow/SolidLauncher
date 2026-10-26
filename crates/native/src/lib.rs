//! Download SolidCraft, check it, install it under the home directory, and open it.

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

pub use http::{check_status, install_latest, Status};
pub use install::{detect, home_dir, launch, remove_old_copies, Installed, RemovedCopies};

/// Sent on every request. No name, email or account id.
pub const USER_AGENT: &str = concat!("SolidLauncher/", env!("CARGO_PKG_VERSION"));
