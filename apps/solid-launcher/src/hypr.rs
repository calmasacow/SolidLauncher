//! Hide and show the SolidLauncher window on Hyprland.
//!
//! Closing the winit window does not unmap it here: the event loop exits before it
//! destroys the Wayland surface, so the window would stay on screen. Moving it to a
//! named special workspace takes it off the monitor and leaves the process running.
//!
//! A move back onto a tiled workspace is applied after `hyprctl` returns, and that
//! move drops the float. Size and float are set only after the window has arrived.

use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

const CLASS: &str = "class:^solid-launcher$";
const SPECIAL: &str = "special:solid-launcher";
const WINDOW_CLASS: &str = "solid-launcher";
const WIDTH: i64 = 600;
const HEIGHT: i64 = 400;

enum PlaceJob {
    Reveal,
    Conceal,
}

/// Hyprland calls go through here, off the window thread.
///
/// A `hyprctl` round trip from the window thread stops that thread reading Wayland.
/// Hyprland then decides the app is not responding and shows its dialog.
pub struct Place {
    tx: Sender<PlaceJob>,
}

impl Place {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || place_worker(rx));
        Self { tx }
    }

    pub fn reveal(&self) {
        let _ = self.tx.send(PlaceJob::Reveal);
    }

    pub fn conceal(&self) {
        let _ = self.tx.send(PlaceJob::Conceal);
    }
}

fn place_worker(rx: Receiver<PlaceJob>) {
    while let Ok(mut job) = rx.recv() {
        while let Ok(newer) = rx.try_recv() {
            job = newer;
        }
        match job {
            PlaceJob::Reveal => {
                let _ = reveal_launcher();
            }
            PlaceJob::Conceal => {
                let _ = conceal_launcher();
            }
        }
    }
}

fn conceal_launcher() -> bool {
    if !eval(&format!(
        r#"hl.dispatch(hl.dsp.window.move({{ workspace = "{SPECIAL}", follow = false, window = "{CLASS}" }}))"#
    )) {
        return false;
    }
    wait_until(|snap| is_special(&snap.workspace), 8)
}

fn reveal_launcher() -> bool {
    // Moving onto a tiled workspace drops the float after this call returns, which flashes
    // a stretched window. The listener floats it again inside the compositor, on that move.
    ensure_hook();
    if !eval(&format!(
        r#"hl.dispatch(hl.dsp.window.move({{ workspace = hl.get_active_workspace(), follow = true, window = "{CLASS}" }}))"#
    )) {
        return false;
    }
    // The compositor tiles the window when this move lands. Wait for that, then float it.
    if !wait_until(|snap| !is_special(&snap.workspace), 15) {
        return false;
    }
    for _ in 0..4 {
        if !place_floating() {
            return false;
        }
        if wait_until(is_placed, 4) {
            return true;
        }
    }
    matches!(launcher_window(), Ok(snap) if is_placed(&snap))
}

fn ensure_hook() {
    let _ = eval(&format!(
        r#"if not _G.solid_launcher_hooked then
  _G.solid_launcher_hooked = true
  hl.on("window.move_to_workspace", function(w, ws)
    if w ~= nil and w.class == "{WINDOW_CLASS}" then
      local name = ""
      if ws ~= nil and ws.name ~= nil then name = tostring(ws.name) end
      if string.sub(name, 1, 8) ~= "special:" then
        hl.dispatch(hl.dsp.window.float({{ action = "set", window = w }}))
        hl.dispatch(hl.dsp.window.resize({{ x = {WIDTH}, y = {HEIGHT}, window = w }}))
        hl.dispatch(hl.dsp.window.center({{ window = w }}))
      end
    end
  end)
end"#
    ));
}

fn place_floating() -> bool {
    eval(&format!(
        r#"hl.dispatch(hl.dsp.window.float({{ action = "set", window = "{CLASS}" }}))
hl.dispatch(hl.dsp.window.resize({{ x = {WIDTH}, y = {HEIGHT}, window = "{CLASS}" }}))
hl.dispatch(hl.dsp.window.center({{ window = "{CLASS}" }}))
hl.dispatch(hl.dsp.focus({{ window = "{CLASS}" }}))"#
    ))
}

/// The compositor applies a move after `hyprctl` returns. Look a few times.
fn wait_until(want: impl Fn(&WindowSnap) -> bool, tries: u32) -> bool {
    for attempt in 0..tries {
        if let Ok(snap) = launcher_window() {
            if want(&snap) {
                return true;
            }
        }
        if attempt + 1 < tries {
            thread::sleep(Duration::from_millis(40));
        }
    }
    false
}

fn is_placed(snap: &WindowSnap) -> bool {
    !is_special(&snap.workspace) && snap.floating && snap.width == WIDTH && snap.height == HEIGHT
}

#[derive(Debug)]
struct WindowSnap {
    floating: bool,
    width: i64,
    height: i64,
    workspace: String,
}

fn launcher_window() -> Result<WindowSnap, ()> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .map_err(|_| ())?;
    if !output.status.success() {
        return Err(());
    }
    let text = String::from_utf8(output.stdout).map_err(|_| ())?;
    parse_launcher(&text)
}

fn parse_launcher(json: &str) -> Result<WindowSnap, ()> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| ())?;
    let list = value.as_array().ok_or(())?;
    for window in list {
        if window.get("class").and_then(serde_json::Value::as_str) != Some(WINDOW_CLASS) {
            continue;
        }
        let size = window.get("size").and_then(serde_json::Value::as_array);
        let width = size
            .and_then(|pair| pair.first())
            .map(json_i64)
            .unwrap_or(0);
        let height = size.and_then(|pair| pair.get(1)).map(json_i64).unwrap_or(0);
        let floating = window
            .get("floating")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let workspace = window
            .get("workspace")
            .and_then(|space| space.get("name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        return Ok(WindowSnap {
            floating,
            width,
            height,
            workspace,
        });
    }
    Err(())
}

fn json_i64(value: &serde_json::Value) -> i64 {
    value.as_i64().unwrap_or_else(|| {
        value
            .as_u64()
            .and_then(|number| i64::try_from(number).ok())
            .unwrap_or(0)
    })
}

fn eval(code: &str) -> bool {
    Command::new("hyprctl")
        .args(["eval", code])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn is_special(name: &str) -> bool {
    name == SPECIAL || name.starts_with("special:")
}

#[cfg(test)]
mod tests {
    use super::{is_special, parse_launcher};

    #[test]
    fn a_scratchpad_name_counts_as_hidden() {
        assert!(is_special("special:solid-launcher"));
        assert!(is_special("special:other"));
        assert!(!is_special("1"));
        assert!(!is_special("solid-launcher"));
    }

    #[test]
    fn the_launcher_is_read_from_the_client_list() {
        let json = r#"[{"class":"other","floating":false,"size":[10,10],"workspace":{"name":"1"}},{"class":"solid-launcher","floating":true,"size":[600,400],"workspace":{"id":-98,"name":"special:solid-launcher"}}]"#;
        let snap = parse_launcher(json);
        assert!(snap.is_ok(), "{snap:?}");
        let snap = snap.unwrap_or_else(|_| super::WindowSnap {
            floating: false,
            width: 0,
            height: 0,
            workspace: String::new(),
        });
        assert!(snap.floating);
        assert_eq!(snap.width, 600);
        assert_eq!(snap.height, 400);
        assert!(is_special(&snap.workspace));
        assert!(parse_launcher("[]").is_err());
        assert!(parse_launcher("not json").is_err());
    }
}
