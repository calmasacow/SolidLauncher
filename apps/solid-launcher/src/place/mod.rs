//! Take the window off the screen and bring it back.
//!
//! Closing the window leaves SolidLauncher running in the status bar, so something has to
//! unmap the surface without ending the process. On nearly every desktop the toolkit does
//! it: `ViewportCommand::Visible(false)` hides the window and `Visible(true)` brings it
//! back. Hyprland is the exception — the event loop exits before it destroys the Wayland
//! surface, so the window stays on screen — and there the compositor is asked to move the
//! window to a special workspace instead.
//!
//! That workspace is not on a monitor, so Hyprland never sends it a frame. Painting there
//! waits inside the buffer swap, on the same thread that has to answer Hyprland's pings,
//! and the "not responding" dialog appears. The move therefore waits until painting has
//! stopped, and a show moves the window back before the next paint.
//!
//! [`Place::hides_the_window`] says which of the two is in use, so the caller knows whether
//! to send the viewport command itself.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

mod hypr;

/// The closing click still has a frame to present. Hide only after that swap has
/// returned. A swap of a window Hyprland is not showing never finishes.
const HIDE_AFTER_FRAME: Duration = Duration::from_millis(400);

enum Job {
    Reveal,
    /// Show the window, then let the caller know the move has finished.
    RevealWait(Sender<()>),
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
#[derive(Clone)]
pub struct Place {
    backend: Backend,
    tx: Option<Sender<Job>>,
    /// Bumped at the start of each window frame, before that frame can swap.
    frames: Arc<AtomicU64>,
    /// Cleared when a show wins over a hide that is still waiting out its last frame.
    wants_hidden: Arc<AtomicBool>,
}

impl Place {
    pub fn start() -> Self {
        let backend = backend();
        let frames = Arc::new(AtomicU64::new(0));
        let wants_hidden = Arc::new(AtomicBool::new(false));
        let tx = match backend {
            Backend::Hyprland => {
                let (tx, rx) = mpsc::channel();
                let frames = Arc::clone(&frames);
                let wants_hidden = Arc::clone(&wants_hidden);
                thread::spawn(move || worker(rx, frames, wants_hidden));
                Some(tx)
            }
            Backend::Toolkit => None,
        };
        Self {
            backend,
            tx,
            frames,
            wants_hidden,
        }
    }

    /// The window thread is inside a frame. Called before that frame can swap.
    pub fn begin_frame(&self) {
        self.frames.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether the compositor hides the window, so the caller should not also hide it.
    ///
    /// `false` means the caller sends `Visible(false)` and `Visible(true)` itself.
    pub fn hides_the_window(&self) -> bool {
        self.backend == Backend::Hyprland
    }

    pub fn reveal(&self) {
        self.wants_hidden.store(false, Ordering::SeqCst);
        self.send(Job::Reveal);
    }

    /// Block the caller, not the window thread, until the window is back on a monitor.
    ///
    /// The next paint has to happen after this returns. Painting first would swap a
    /// buffer on the hidden workspace and miss Hyprland's pings.
    pub fn reveal_wait(&self) {
        self.wants_hidden.store(false, Ordering::SeqCst);
        let Some(tx) = &self.tx else {
            return;
        };
        let (done_tx, done_rx) = mpsc::channel();
        if tx.send(Job::RevealWait(done_tx)).is_err() {
            return;
        }
        let _ = done_rx.recv_timeout(Duration::from_secs(2));
    }

    pub fn conceal(&self) {
        self.wants_hidden.store(true, Ordering::SeqCst);
        self.send(Job::Conceal);
    }

    fn send(&self, job: Job) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(job);
        }
    }
}

/// Hide only once painting has gone quiet and nobody has asked to show the window since.
fn hide_when_idle(wants_hidden: bool, frames_seen: u64, frames_now: u64) -> bool {
    wants_hidden && frames_seen == frames_now
}

fn worker(rx: Receiver<Job>, frames: Arc<AtomicU64>, wants_hidden: Arc<AtomicBool>) {
    while let Ok(job) = rx.recv() {
        run_job(&rx, &frames, &wants_hidden, job);
    }
}

fn run_job(rx: &Receiver<Job>, frames: &AtomicU64, wants_hidden: &AtomicBool, job: Job) {
    match job {
        Job::Reveal => {
            let _ = hypr::reveal();
        }
        Job::RevealWait(done) => {
            wants_hidden.store(false, Ordering::SeqCst);
            let _ = hypr::reveal();
            let _ = done.send(());
        }
        Job::Conceal => loop {
            let seen = frames.load(Ordering::Relaxed);
            thread::sleep(HIDE_AFTER_FRAME);
            match rx.try_recv() {
                Ok(newer) => {
                    run_job(rx, frames, wants_hidden, newer);
                    break;
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {
                    if hide_when_idle(
                        wants_hidden.load(Ordering::SeqCst),
                        seen,
                        frames.load(Ordering::Relaxed),
                    ) {
                        let _ = hypr::conceal();
                        break;
                    }
                    if !wants_hidden.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{backend, hide_when_idle, Backend, Place};

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

    #[test]
    fn a_hide_waits_until_painting_stops_and_a_show_cancels_it() {
        assert!(!hide_when_idle(true, 4, 5));
        assert!(hide_when_idle(true, 5, 5));
        assert!(!hide_when_idle(false, 5, 5));
    }
}
