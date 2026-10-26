//! One SolidLauncher per session on Windows, over a loopback socket.
//!
//! `std` has no named pipes, so the running instance listens on `127.0.0.1` on a port the
//! system picks, and writes that port and a fresh secret to a file only this user can
//! reach. A second start reads the file and sends the secret; anything else that finds the
//! port is ignored.
//!
//! The secret matters because any process on the machine can reach a loopback port. All it
//! guards is "show the window", so a leak is a nuisance rather than a way in, but the
//! handoff is cheap to do properly.

use std::io::{BufRead, BufReader, Read as _, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::Claim;

/// Long enough that guessing it is not worth anyone's time.
const SECRET_BYTES: usize = 32;
/// A handoff is one short line to a local port; longer than this means it is not ours.
const MAX_LINE: u64 = 256;

pub(super) fn claim(wake: impl Fn() + Send + 'static) -> Claim {
    let Some(path) = state_path() else {
        return Claim::Alone;
    };
    // A live instance answers; a file left behind by a crash does not.
    if let Some((port, secret)) = read_state(&path) {
        if hand_off(port, &secret).is_ok() {
            return Claim::Secondary;
        }
    }
    match listen(&path, wake) {
        Ok(claim) => claim,
        Err(err) => {
            eprintln!("SolidLauncher: single-instance socket: {err}");
            Claim::Alone
        }
    }
}

fn listen(path: &std::path::Path, wake: impl Fn() + Send + 'static) -> std::io::Result<Claim> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    let port = match listener.local_addr()? {
        SocketAddr::V4(addr) => addr.port(),
        SocketAddr::V6(addr) => addr.port(),
    };
    let secret = secret();
    write_state(path, port, &secret)?;

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(conn) = conn else {
                continue;
            };
            if !is_ours(&conn, &secret) {
                continue;
            }
            if tx.send(()).is_err() {
                break;
            }
            wake();
        }
    });
    Ok(Claim::Primary { shows: rx })
}

/// Whether this connection gave the secret. A caller that stalls is dropped.
fn is_ours(conn: &TcpStream, secret: &str) -> bool {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
    let Ok(peer) = conn.peer_addr() else {
        return false;
    };
    if !peer.ip().is_loopback() {
        return false;
    }
    let mut line = String::new();
    let Ok(reader) = conn.try_clone() else {
        return false;
    };
    if BufReader::new(reader.take(MAX_LINE))
        .read_line(&mut line)
        .is_err()
    {
        return false;
    }
    matches(line.trim(), secret)
}

/// Compare without returning early, so timing says nothing about how much matched.
fn matches(given: &str, secret: &str) -> bool {
    let (given, secret) = (given.as_bytes(), secret.as_bytes());
    if given.len() != secret.len() {
        return false;
    }
    let mut differences = 0_u8;
    for (a, b) in given.iter().zip(secret) {
        differences |= a ^ b;
    }
    differences == 0
}

fn hand_off(port: u16, secret: &str) -> std::io::Result<()> {
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let mut stream = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(format!("{secret}\n").as_bytes())?;
    stream.flush()
}

fn secret() -> String {
    let mut bytes = [0_u8; SECRET_BYTES];
    if getrandom::fill(&mut bytes).is_err() {
        // Without the system generator there is nothing better to offer; the port is still
        // picked by the system and the only command is "show the window".
        return String::new();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `<port>\n<secret>\n`, under this user's own local app data.
fn state_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty())?;
    Some(PathBuf::from(base).join("SolidLauncher").join("instance"))
}

fn read_state(path: &std::path::Path) -> Option<(u16, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let port = lines.next()?.trim().parse().ok()?;
    let secret = lines.next()?.trim().to_string();
    (port != 0 && !secret.is_empty()).then_some((port, secret))
}

fn write_state(path: &std::path::Path, port: u16, secret: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{port}\n{secret}\n"))
}
