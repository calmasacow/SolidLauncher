//! The desktop notification that says a newer SolidCraft is out.
//!
//! One call for all platforms: `notify-rust` speaks D-Bus on Linux and the BSDs, the user
//! notification centre on macOS, and toasts on Windows. A notification that cannot be shown
//! is reported on stderr and nothing else — missing it is not worth interrupting anyone
//! over, and the window says the same thing.

use std::thread;

/// Shown as the sender, and on Windows as the toast's application.
const APP_NAME: &str = "SolidLauncher";

/// Tell the user that `latest` is out while `installed` is what they have.
///
/// Returns immediately: the notification is posted from its own thread, because some
/// desktops answer slowly and the window thread must keep drawing.
pub fn update_available(latest: &str, installed: &str) {
    let summary = format!("SolidCraft {latest} is available");
    let body = format!("{installed} is installed. Open SolidLauncher to update.");
    thread::spawn(move || show(&summary, &body));
}

/// Post a notification and wait for the desktop to take it.
///
/// Unlike [`update_available`] this does not return before the notification is away,
/// because the one caller that needs it is on its way out of the process and a thread
/// would be killed before it posted. A notification that cannot be shown is reported on
/// stderr and otherwise ignored.
pub fn show(summary: &str, body: &str) {
    let mut notification = notify_rust::Notification::new();
    notification.appname(APP_NAME).summary(summary).body(body);
    if let Some(icon) = icon() {
        notification.icon(&icon);
    }
    if let Err(err) = notification.show() {
        eprintln!("SolidLauncher: could not show a notification ({err})");
    }
}

/// The icon to show beside the notification, where the platform takes one from us.
///
/// On an XDG desktop the installed PNG is used when it is there, and the themed name
/// otherwise. macOS and Windows take the icon from the application itself, so there is
/// nothing to pass.
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn icon() -> Option<String> {
    let home = solid_launcher_native::home_dir().ok()?;
    let path = home.join(".local/share/icons/hicolor/128x128/apps/solid-launcher.png");
    Some(if path.is_file() {
        path.display().to_string()
    } else {
        "solid-launcher".to_string()
    })
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
)))]
fn icon() -> Option<String> {
    None
}
