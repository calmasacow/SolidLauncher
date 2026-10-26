//! Take the window off the screen and bring it back.
//!
//! Closing the window leaves SolidLauncher running in the status bar, so something has to
//! unmap the surface without ending the process. On nearly every desktop the toolkit does
//! it: `ViewportCommand::Visible(false)` hides the window and `Visible(true)` brings it
//! back. Hyprland is the exception — the event loop exits before it destroys the Wayland
//! surface, so the window stays on screen — and there the compositor is asked to move the
//! window to a special workspace instead.
//!
//! [`Place::hides_the_window`] says which of the two is in use, so the caller knows whether
//! to send the viewport command itself.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

mod hypr;

enum Job {
    Reveal,
    Conceal,
}

/// How this session takes the window off the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    /// Hyprland moves the window to a special workspace.
    Hyprland,
    /// Everyone else: the caller hides the surface through the toolkit.
    Toolkit,
}

/// Hyprland sets this for every client it starts, and nothing else does.
fn running_on_hyprland() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|value| !value.is_empty())
}

fn backend() -> Backend {
    if cfg!(target_os = "linux") && running_on_hyprland() {
        Backend::Hyprland
    } else {
        Backend::Toolkit
    }
}

/// Moves the window on and off the screen.
///
/// Compositor calls run on their own thread. A `hyprctl` round trip from the window thread
/// stops that thread reading Wayland, and Hyprland then decides the app is not responding
/// and shows its dialog.
pub struct Place {
    backend: Backend,
    tx: Option<Sender<Job>>,
}

impl Place {
    pub fn start() -> Self {
        let backend = backend();
        let tx = match backend {
            Backend::Hyprland => {
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || worker(rx));
                Some(tx)
            }
            Backend::Toolkit => None,
        };
        Self { backend, tx }
    }

    /// Whether the compositor hides the window, so the caller should not also hide it.
    ///
    /// `false` means the caller sends `Visible(false)` and `Visible(true)` itself.
    pub fn hides_the_window(&self) -> bool {
        self.backend == Backend::Hyprland
    }

    pub fn reveal(&self) {
        self.send(Job::Reveal);
    }

    pub fn conceal(&self) {
        self.send(Job::Conceal);
    }

    fn send(&self, job: Job) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(job);
        }
    }
}

fn worker(rx: Receiver<Job>) {
    while let Ok(mut job) = rx.recv() {
        // A burst of clicks only needs the last one acted on.
        while let Ok(newer) = rx.try_recv() {
            job = newer;
        }
        let _ = match job {
            Job::Reveal => hypr::reveal(),
            Job::Conceal => hypr::conceal(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{backend, Backend, Place};

    #[test]
    fn a_desktop_that_is_not_hyprland_hides_its_own_window() {
        // The test runner is not a Hyprland session, so this is the toolkit path: the one
        // that was missing before, and that every non-Hyprland desktop now takes.
        assert_eq!(backend(), Backend::Toolkit);
        let place = Place::start();
        assert!(!place.hides_the_window());
        // Without a compositor backend these are no-ops rather than failures.
        place.conceal();
        place.reveal();
    }
}
