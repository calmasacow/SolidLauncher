//! The bar icon. Left click shows or hides the window. The menu can check or quit.

use std::sync::mpsc::{self, Receiver, Sender};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// StatusNotifierItem id. The default is "<pid>-<n>", which the bar would pin as a number.
pub const TRAY_ID: &str = "solid-launcher";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Toggle,
    Show,
    Check,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    rx: Receiver<TrayAction>,
    update_available: bool,
}

fn load_icon() -> Result<tray_icon::Icon, String> {
    // SolidCraft's brand mark. Not covered by this crate's licence: see ATTRIBUTION.md.
    let bytes: &[u8] = include_bytes!("../../../assets/solidcraft-256.png");
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|err| err.to_string())?
        .into_rgba8();
    let (width, height) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), width, height).map_err(|err| err.to_string())
}

/// Names the icon on Linux and the BSDs. Windows and macOS keep tray-icon's own id, because
/// there the id never reaches the user and a title would be drawn beside the icon.
pub fn identity(builder: TrayIconBuilder) -> TrayIconBuilder {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        builder
    } else {
        builder.with_id(TRAY_ID).with_title("SolidLauncher")
    }
}

impl Tray {
    /// `wake` repaints the window, or the tray loop when there is no window.
    /// `on_quit` runs as soon as Quit is picked, in case that window never gets another frame.
    pub fn new(
        wake: impl Fn(TrayAction) + Send + Sync + Clone + 'static,
        on_quit: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let show = MenuItem::new("Show SolidLauncher", true, None);
        let check = MenuItem::new("Check for updates", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        menu.append_items(&[&show, &check, &PredefinedMenuItem::separator(), &quit])
            .map_err(|err| err.to_string())?;
        let icon = identity(TrayIconBuilder::new())
            .with_menu(Box::new(menu))
            .with_icon(load_icon()?)
            .with_icon_as_template(cfg!(target_os = "macos"))
            .with_menu_on_left_click(cfg!(target_os = "macos"))
            .with_tooltip("SolidLauncher")
            .build()
            .map_err(|err| err.to_string())?;
        let (tx, rx): (Sender<TrayAction>, Receiver<TrayAction>) = mpsc::channel();
        let show_id = show.id().clone();
        let check_id = check.id().clone();
        let quit_id = quit.id().clone();
        {
            let tx = tx.clone();
            let wake = wake.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let action = if event.id == show_id {
                    TrayAction::Show
                } else if event.id == check_id {
                    TrayAction::Check
                } else if event.id == quit_id {
                    TrayAction::Quit
                } else {
                    return;
                };
                if matches!(action, TrayAction::Quit) {
                    on_quit();
                }
                let _ = tx.send(action);
                wake(action);
            }));
        }
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = tx.send(TrayAction::Toggle);
                wake(TrayAction::Toggle);
            }
        }));
        Ok(Self {
            icon,
            rx,
            update_available: false,
        })
    }

    pub fn actions(&self) -> Vec<TrayAction> {
        self.rx.try_iter().collect()
    }

    pub fn set_update(&mut self, update_available: bool) {
        if update_available == self.update_available {
            return;
        }
        self.update_available = update_available;
        let tip = if update_available {
            "SolidLauncher · update available"
        } else {
            "SolidLauncher"
        };
        let _ = self.icon.set_tooltip(Some(tip));
    }
}

#[cfg(test)]
mod tests {
    use super::{identity, TRAY_ID};
    use tray_icon::TrayIconBuilder;

    #[test]
    fn linux_tray_id_is_stable() {
        let id = identity(TrayIconBuilder::new()).id().clone();
        if cfg!(any(target_os = "macos", target_os = "windows")) {
            let pid = std::process::id().to_string();
            assert!(id.as_ref().starts_with(&pid), "{id:?}");
        } else {
            assert_eq!(id.as_ref(), TRAY_ID);
        }
    }
}
