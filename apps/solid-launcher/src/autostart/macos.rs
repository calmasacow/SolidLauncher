//! The macOS login entry: a LaunchAgent property list under the user's own Library.
//!
//! `launchd` reads every plist in `~/Library/LaunchAgents` when the user logs in, so the
//! file alone is the whole mechanism. `RunAtLoad` starts it once at login; without
//! `KeepAlive` it is not restarted when the user quits it.

use std::path::{Path, PathBuf};

use solid_launcher_native::home_dir;

/// The agent's label, which is also its file name.
///
/// Taken from the project's GitHub namespace rather than a domain, so it claims nothing
/// that is not ours.
pub(super) const LABEL: &str = "com.github.calmasacow.solidlauncher";

pub(super) fn entry_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

fn entry(exe: &Path, minimized: bool) -> String {
    let mut arguments = format!(
        "        <string>{}</string>\n",
        escape(&exe.display().to_string())
    );
    if minimized {
        arguments.push_str("        <string>--hidden</string>\n");
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20   <key>Label</key>\n\
         \x20   <string>{LABEL}</string>\n\
         \x20   <key>ProgramArguments</key>\n\
         \x20   <array>\n\
         {arguments}\
         \x20   </array>\n\
         \x20   <key>RunAtLoad</key>\n\
         \x20   <true/>\n\
         \x20   <key>ProcessType</key>\n\
         \x20   <string>Interactive</string>\n\
         </dict>\n\
         </plist>\n"
    )
}

/// A path is user data and lands inside an XML element, so the five XML entities go out.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(super) fn write(path: &Path, exe: &Path, minimized: bool) -> Result<(), String> {
    std::fs::write(path, entry(exe, minimized))
        .map_err(|err| format!("could not write {} ({err})", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{entry, entry_path, escape, write, LABEL};
    use std::path::Path;

    #[test]
    fn a_path_with_xml_in_it_cannot_break_the_plist() {
        assert_eq!(
            escape("a&b<c>\"d\"'e'"),
            "a&amp;b&lt;c&gt;&quot;d&quot;&apos;e&apos;"
        );
        let body = entry(Path::new("/Users/a&b/solid-launcher"), true);
        assert!(body.contains("/Users/a&amp;b/solid-launcher"), "{body}");
        assert!(!body.contains("/Users/a&b/"), "{body}");
        assert!(body.contains("<string>--hidden</string>"), "{body}");
    }

    #[test]
    fn the_agent_goes_in_this_users_own_launch_agents_folder() {
        let Ok(path) = entry_path() else {
            // No home directory in this environment; nothing to check.
            return;
        };
        assert!(path.is_absolute(), "{}", path.display());
        assert!(
            path.ends_with(format!("Library/LaunchAgents/{LABEL}.plist")),
            "{}",
            path.display()
        );
    }

    #[test]
    fn the_written_agent_is_a_plist_that_runs_the_program_at_login() {
        let dir =
            std::env::temp_dir().join(format!("solid-launcher-launchagent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(std::fs::create_dir_all(&dir).is_ok());
        let path = dir.join("agent.plist");
        let exe =
            Path::new("/Users/someone/Applications/SolidLauncher.app/Contents/MacOS/SolidLauncher");

        assert!(write(&path, exe, true).is_ok());
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(body.starts_with("<?xml"), "{body}");
        assert!(
            body.contains(&format!("<string>{LABEL}</string>")),
            "{body}"
        );
        assert!(body.contains("<string>/Users/someone/Applications/SolidLauncher.app/Contents/MacOS/SolidLauncher</string>"), "{body}");
        assert!(body.contains("<key>RunAtLoad</key>"), "{body}");
        assert!(body.contains("<string>--hidden</string>"), "{body}");

        // Without "start minimized" the agent passes no arguments at all.
        assert!(write(&path, exe, false).is_ok());
        let shown = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!shown.contains("--hidden"), "{shown}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
