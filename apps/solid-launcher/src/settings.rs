//! The three options in the window. The login entry two of them control is in `autostart`.

use std::path::{Path, PathBuf};

use solid_launcher_native::PackageKind;

use solid_launcher_native::home_dir;

/// How SolidCraft should be installed, when the user has an opinion.
///
/// [`Method::Automatic`] is the default and means two things in order: keep using
/// whatever format is already installed, so an update never silently changes it; and
/// failing that, take what the platform prefers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Method {
    #[default]
    Automatic,
    /// The official tarball, into the home directory. No password.
    Tarball,
    /// One executable file, into the home directory. No password.
    AppImage,
    /// A system package. Asks for a password, and `apt remove` takes it away.
    Deb,
    /// A system package. Asks for a password, and `dnf remove` takes it away.
    Rpm,
}

impl Method {
    /// What `install_latest` should be asked for: `None` is the automatic choice.
    pub fn preferred(self) -> Option<PackageKind> {
        match self {
            Self::Automatic => None,
            Self::Tarball => Some(PackageKind::TarGz),
            Self::AppImage => Some(PackageKind::AppImage),
            Self::Deb => Some(PackageKind::Deb),
            Self::Rpm => Some(PackageKind::Rpm),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Tarball => "Tarball",
            Self::AppImage => "AppImage",
            Self::Deb => "System package (.deb)",
            Self::Rpm => "System package (.rpm)",
        }
    }

    /// The one line under the choice, saying what picking it means.
    pub fn note(self) -> &'static str {
        match self {
            Self::Automatic => "Keeps the format already installed, or picks one for you.",
            Self::Tarball => "Into your home directory, with the command-line tool. No password.",
            Self::AppImage => "One file in your home directory. No password.",
            Self::Deb | Self::Rpm => "For everyone on this computer. Asks for your password.",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Tarball => "tarball",
            Self::AppImage => "appimage",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "tarball" => Self::Tarball,
            "appimage" => Self::AppImage,
            "deb" => Self::Deb,
            "rpm" => Self::Rpm,
            _ => Self::Automatic,
        }
    }

    /// The methods offered on this platform, best first.
    pub fn choices() -> &'static [Method] {
        if cfg!(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        )) {
            &[
                Self::Automatic,
                Self::Tarball,
                Self::AppImage,
                Self::Deb,
                Self::Rpm,
            ]
        } else {
            // Windows and macOS each publish one thing the launcher installs, so there
            // is nothing to choose between.
            &[Self::Automatic]
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub start_on_login: bool,
    pub start_minimized: bool,
    pub remove_old_versions: bool,
    pub method: Method,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_on_login: true,
            start_minimized: true,
            remove_old_versions: false,
            method: Method::Automatic,
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
            // An unknown or missing name reads as Automatic rather than failing: a
            // settings file written by a newer version must not stop this one starting.
            method: value
                .get("install_method")
                .and_then(serde_json::Value::as_str)
                .map_or(defaults.method, Method::from_name),
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
            "install_method": self.method.name(),
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
    use super::{Method, Settings};

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
    fn a_saved_method_comes_back() {
        for method in [
            Method::Automatic,
            Method::Tarball,
            Method::AppImage,
            Method::Deb,
            Method::Rpm,
        ] {
            assert_eq!(Method::from_name(method.name()), method);
        }
    }

    #[test]
    fn an_unknown_method_reads_as_automatic() {
        // A settings file written by a newer version must not stop this one.
        assert_eq!(Method::from_name("snap"), Method::Automatic);
        assert_eq!(Method::from_name(""), Method::Automatic);
    }

    #[test]
    fn automatic_asks_for_no_particular_format() {
        assert!(Method::Automatic.preferred().is_none());
        assert!(Method::AppImage.preferred().is_some());
    }

    #[test]
    fn every_offered_method_is_named_and_explained() {
        for method in Method::choices() {
            assert!(!method.label().is_empty());
            assert!(!method.note().is_empty());
        }
        assert!(Method::choices().contains(&Method::Automatic));
    }
}
