//! The three options in the window, and the login entry they control.

use std::path::{Path, PathBuf};

use solid_launcher_native::home_dir;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub start_on_login: bool,
    pub start_minimized: bool,
    pub remove_old_versions: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_on_login: true,
            start_minimized: true,
            remove_old_versions: false,
        }
    }
}

impl Settings {
    pub fn load() -> (Self, Option<String>) {
        let path = match settings_path() {
            Ok(path) => path,
            Err(err) => return (Self::default(), Some(err)),
        };
        if !path.is_file() {
            let settings = Self::default();
            let warning = settings.save().err();
            return (settings, warning);
        }
        match Self::read(&path) {
            Ok(settings) => (settings, None),
            Err(_) => (
                Self::default(),
                Some(
                    "SolidLauncher could not read its options, so the usual choices are in use."
                        .to_string(),
                ),
            ),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path()?;
        self.write(&path)
    }

    fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("could not read {} ({err})", path.display()))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("could not read {} ({err})", path.display()))?;
        let defaults = Self::default();
        Ok(Self {
            start_on_login: value
                .get("start_on_login")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(defaults.start_on_login),
            start_minimized: value
                .get("start_minimized")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(defaults.start_minimized),
            remove_old_versions: value
                .get("remove_old_versions")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(defaults.remove_old_versions),
        })
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("could not create {} ({err})", parent.display()))?;
        }
        let body = serde_json::json!({
            "start_on_login": self.start_on_login,
            "start_minimized": self.start_minimized,
            "remove_old_versions": self.remove_old_versions,
        });
        let text = serde_json::to_string_pretty(&body)
            .map_err(|err| format!("could not store the options ({err})"))?;
        std::fs::write(path, text + "\n")
            .map_err(|err| format!("could not write {} ({err})", path.display()))
    }
}

/// Create or remove `~/.config/autostart/solid-launcher.desktop` to match the options.
pub fn apply(settings: &Settings) -> Result<(), String> {
    let exe =
        std::env::current_exe().map_err(|err| format!("could not find SolidLauncher ({err})"))?;
    apply_at(&autostart_path()?, &exe, settings)
}

pub fn apply_at(path: &Path, exe: &Path, settings: &Settings) -> Result<(), String> {
    if !settings.start_on_login {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!("could not turn off start on login ({err})")),
        }
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("could not create {} ({err})", parent.display()))?;
        }
        let body = autostart_entry(exe, settings.start_minimized);
        std::fs::write(path, body)
            .map_err(|err| format!("could not write {} ({err})", path.display()))
    }
}

pub fn autostart_entry(exe: &Path, minimized: bool) -> String {
    let mut exec = exe.display().to_string();
    if exec.contains([' ', '\t', '"']) {
        exec = format!("\"{}\"", exec.replace('"', "\\\""));
    }
    if minimized {
        exec = format!("{exec} --hidden");
    }
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=SolidLauncher\n\
         Comment=Install, update and open SolidCraft\n\
         Exec={exec}\n\
         Icon=solid-launcher\n\
         Terminal=false\n\
         StartupNotify=false\n\
         StartupWMClass=solid-launcher\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

fn settings_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".config/solid-launcher/settings.json"))
}

fn autostart_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".config/autostart/solid-launcher.desktop"))
}

#[cfg(test)]
mod tests {
    use super::{apply_at, autostart_entry, Settings};

    #[test]
    fn missing_keys_keep_the_usual_choices() {
        let dir =
            std::env::temp_dir().join(format!("solid-launcher-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(std::fs::create_dir_all(&dir).is_ok());
        let path = dir.join("settings.json");
        assert!(std::fs::write(&path, "{}\n").is_ok());
        let read = Settings::read(&path);
        assert!(read.is_ok(), "{read:?}");
        let settings = read.unwrap_or_default();
        assert!(settings.start_on_login);
        assert!(settings.start_minimized);
        assert!(!settings.remove_old_versions);

        let mut changed = settings;
        changed.start_minimized = false;
        changed.remove_old_versions = true;
        assert!(changed.write(&path).is_ok());
        let read = Settings::read(&path);
        assert!(read.is_ok(), "{read:?}");
        let again = read.unwrap_or_default();
        assert!(again.start_on_login);
        assert!(!again.start_minimized);
        assert!(again.remove_old_versions);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn login_entry_follows_the_two_start_options() {
        let dir = std::env::temp_dir().join(format!("solid-launcher-auto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("autostart.desktop");
        let exe = std::path::Path::new("/home/calmasacow/.local/bin/solid-launcher");
        let mut settings = Settings::default();
        assert!(apply_at(&path, exe, &settings).is_ok());
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(!text.is_empty());
        assert!(text.contains("Exec=/home/calmasacow/.local/bin/solid-launcher --hidden\n"));
        assert_eq!(text, autostart_entry(exe, true));

        settings.start_minimized = false;
        assert!(apply_at(&path, exe, &settings).is_ok());
        let shown = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(shown.contains("Exec=/home/calmasacow/.local/bin/solid-launcher\n"));
        assert!(!shown.contains("--hidden"));

        settings.start_on_login = false;
        assert!(apply_at(&path, exe, &settings).is_ok());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
