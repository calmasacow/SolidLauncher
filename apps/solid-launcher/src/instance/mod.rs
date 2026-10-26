//! One SolidLauncher per session. A second start asks the first to show its window.
//!
//! Unix has a socket whose file permissions keep other users out. Windows has no such
//! socket in `std`, so it listens on the loopback interface and checks a secret written
//! beside it; see [`windows`].

use std::sync::mpsc::Receiver;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as backend;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as backend;

pub enum Claim {
    /// This process owns the tray.
    Primary { shows: Receiver<()> },
    /// Another SolidLauncher is already running and was asked to show its window.
    Secondary,
    /// The channel could not be taken. Run anyway, without the single-instance handoff.
    Alone,
}

/// Become the one SolidLauncher, or hand off to the one already running.
///
/// `wake` is called from the listener thread whenever another start asks for the window.
pub fn claim(wake: impl Fn() + Send + 'static) -> Claim {
    backend::claim(wake)
}
