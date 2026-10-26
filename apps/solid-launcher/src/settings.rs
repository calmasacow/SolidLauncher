//! The three options in the window. The login entry two of them control is in `autostart`.

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

fn settings_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".config/solid-launcher/settings.json"))
}

#[cfg(test)]
mod tests {
    use super::Settings;

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
}
