//! One SolidLauncher in the session. A second start asks the first to show its window.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

pub enum Claim {
    /// This process owns the tray.
    Primary { shows: Receiver<()> },
    /// Another SolidLauncher is already running and was asked to show its window.
    Secondary,
    /// The socket could not be taken. Run anyway, without the single-instance handoff.
    Alone,
}

pub fn claim(wake: impl Fn() + Send + 'static) -> Claim {
    let Some(path) = socket_path() else {
        return Claim::Alone;
    };
    match bind_or_hand_off(&path, wake) {
        Ok(claim) => claim,
        Err(err) => {
            eprintln!("SolidLauncher: single-instance socket: {err}");
            Claim::Alone
        }
    }
}

fn bind_or_hand_off(
    path: &std::path::Path,
    wake: impl Fn() + Send + 'static,
) -> std::io::Result<Claim> {
    match std::os::unix::net::UnixListener::bind(path) {
        Ok(listener) => Ok(listen(listener, wake)),
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            if hand_off(path).is_ok() {
                return Ok(Claim::Secondary);
            }
            // The file is left behind after a crash. A live listener answers `hand_off`.
            let _ = std::fs::remove_file(path);
            let listener = std::os::unix::net::UnixListener::bind(path)?;
            Ok(listen(listener, wake))
        }
        Err(err) => Err(err),
    }
}

fn listen(listener: std::os::unix::net::UnixListener, wake: impl Fn() + Send + 'static) -> Claim {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else {
                continue;
            };
            let mut buf = [0_u8; 16];
            let _ = conn.read(&mut buf);
            if tx.send(()).is_err() {
                break;
            }
            wake();
        }
    });
    Claim::Primary { shows: rx }
}

fn hand_off(path: &std::path::Path) -> std::io::Result<()> {
    let mut stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(b"show\n")?;
    Ok(())
}

/// The socket stays in the runtime dir after exit. The next start replaces it when nobody answers.
/// Unlinking from `Drop` can delete the replacement a new process just bound.
fn socket_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty());
    let dir = dir.map_or_else(std::env::temp_dir, PathBuf::from);
    if std::fs::create_dir_all(&dir).is_err() && !dir.is_dir() {
        return None;
    }
    Some(dir.join("solid-launcher.sock"))
}
