//! SolidLauncher window: the installed SolidCraft version, the latest release, and the buttons
//! that install, update or open it.
//!
//! Login starts it with `--hidden`, which never maps a window. Closing the window returns it
//! to the bar; see `place` for how it is taken off the screen. Quit, in the tray menu, exits.

// A window application, not a console one: without this Windows opens a terminal behind
// the launcher's own window. Only in release, so `--check` and `--help` still print while
// developing; in a release build on Windows they write to a console that is not there.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod autostart;
mod instance;
mod notify;
mod place;
mod settings;
mod startup_error;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use eframe::egui;
use solid_launcher_native::{
    check_status, detect, home_dir, install_latest, keeps_old_versions, launch, list, remove,
    remove_old_copies, Installed, Installs, Status,
};

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "SolidLauncher installs, updates and opens SolidCraft.\n\n\
             \x20 solid-launcher           open the window\n\
             \x20 solid-launcher --hidden  start in the bar, with no window\n\
             \x20 solid-launcher --check   print the installed and latest versions"
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--check") {
        // --check runs before the window, so it reads the saved choice itself.
        let (settings, _) = settings::Settings::load();
        match check_status(settings.method.preferred()) {
            Ok(status) => {
                println!(
                    "installed: {}",
                    status.installed.as_deref().unwrap_or("not installed")
                );
                println!("latest:    {}", status.latest);
                println!("asset:     {}", status.asset);
                println!(
                    "update:    {}",
                    if status.update_available { "yes" } else { "no" }
                );
                return Ok(());
            }
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }

    let hidden = args.iter().any(|arg| arg == "--hidden");
    // Nothing here prints anywhere the person can see: a release build on Windows has no
    // console, and a Start Menu or desktop entry has none either. So a failure to open the
    // window is written down and notified rather than left to look like nothing happening.
    if let Err(err) = run_gui(hidden) {
        startup_error::report(&err.to_string());
        return Err(err);
    }
    Ok(())
}

fn run_gui(hidden: bool) -> eframe::Result<()> {
    let (wake_tx, wake_rx) = mpsc::sync_channel(1);
    let waker = Arc::new(Waker {
        window: Mutex::new(None),
        wake_tx,
        quit_pending: AtomicBool::new(false),
    });
    let mut launcher = match instance::claim({
        let waker = Arc::clone(&waker);
        move || waker.wake()
    }) {
        instance::Claim::Secondary => return Ok(()),
        instance::Claim::Primary { shows } => Launcher::new(Arc::clone(&waker), shows),
        instance::Claim::Alone => {
            let (_tx, shows) = mpsc::channel();
            Launcher::new(Arc::clone(&waker), shows)
        }
    };

    let tray_waker = Arc::clone(&waker);
    let quit_waker = Arc::clone(&waker);
    match tray::Tray::new(
        move || tray_waker.wake(),
        move || Waker::quit_from_tray(&quit_waker),
    ) {
        Ok(icon) => launcher.tray = Some(icon),
        Err(err) => eprintln!("SolidLauncher: no tray icon: {err}"),
    }

    let mut open_window = !start_in_tray(hidden, launcher.tray.is_some());
    loop {
        if open_window {
            let result = {
                let waker = Arc::clone(&waker);
                let launcher = &mut launcher;
                eframe::run_native(
                    "SolidLauncher",
                    native_options(),
                    Box::new(move |cc| {
                        style(cc);
                        waker.set_window(Some(cc.egui_ctx.clone()));
                        Ok(Box::new(Shell {
                            launcher,
                            quitting: false,
                        }))
                    }),
                )
            };
            waker.set_window(None);
            result?;
            if !std::mem::take(&mut launcher.closed_to_tray) {
                return Ok(());
            }
        }
        if !launcher.wait_in_tray(&wake_rx) {
            return Ok(());
        }
        open_window = true;
    }
}

fn native_options() -> eframe::NativeOptions {
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/solidcraft-256.png"));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("SolidLauncher")
        .with_app_id("solid-launcher")
        .with_inner_size([600.0, 400.0])
        .with_min_inner_size([480.0, 360.0]);
    if let Ok(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    // `run_and_return` makes closing the window come back here, so the bar icon stays.
    eframe::NativeOptions {
        viewport,
        run_and_return: true,
        ..Default::default()
    }
}

/// `--hidden` stays in the bar only when a tray icon exists to come back from.
fn start_in_tray(hidden: bool, tray_ok: bool) -> bool {
    hidden && tray_ok
}

struct Waker {
    window: Mutex<Option<egui::Context>>,
    wake_tx: SyncSender<()>,
    quit_pending: AtomicBool,
}

impl Waker {
    fn wake(&self) {
        if let Some(ctx) = self
            .window
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            ctx.request_repaint();
        }
        let _ = self.wake_tx.try_send(());
    }

    fn set_window(&self, ctx: Option<egui::Context>) {
        *self.window.lock().unwrap_or_else(PoisonError::into_inner) = ctx;
    }

    /// A minimized Wayland window can stop receiving frames, so Quit must not wait on one forever.
    fn quit_from_tray(this: &Arc<Self>) {
        this.quit_pending.store(true, Ordering::SeqCst);
        this.wake();
        let this = Arc::clone(this);
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(2));
            if this.quit_pending.swap(false, Ordering::SeqCst) {
                eprintln!("SolidLauncher: the window did not close; exiting");
                std::process::exit(0);
            }
        });
    }

    fn quit_seen(&self) {
        self.quit_pending.store(false, Ordering::SeqCst);
    }
}

enum Event {
    Checked(Status),
    Progress {
        received: u64,
        total: u64,
    },
    Installed(String),
    Failed(String),
    /// The hourly check is due. The launch check is separate and already happened.
    Hourly,
    /// A Manage copies deletion finished: what went, and what would not.
    Removed {
        removed: Vec<String>,
        errors: Vec<String>,
    },
}

enum Request {
    Toggle,
    Show,
    Check,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayStep {
    Stay,
    Open,
    Quit,
}

fn tray_step(requests: &[Request]) -> TrayStep {
    if requests
        .iter()
        .any(|request| matches!(request, Request::Quit))
    {
        TrayStep::Quit
    } else if requests
        .iter()
        .any(|request| matches!(request, Request::Show | Request::Toggle))
    {
        TrayStep::Open
    } else {
        TrayStep::Stay
    }
}

struct Launcher {
    waker: Arc<Waker>,
    tx: std::sync::mpsc::Sender<Event>,
    rx: Receiver<Event>,
    shows: Receiver<()>,
    tray: Option<tray::Tray>,
    status: Option<Status>,
    installed_label: Option<String>,
    /// The SolidCraft on this machine, whoever installed it. Kept whole rather than as a
    /// path: opening a Flatpak and sweeping old copies both depend on how it got here.
    installed: Option<Installed>,
    busy: bool,
    headline: String,
    error: Option<String>,
    progress: Option<(u64, u64)>,
    /// The window closed so the launcher could stay in the bar.
    closed_to_tray: bool,
    settings: settings::Settings,
    options_open: bool,
    /// The Manage copies sheet, and what it last found. Loaded when it is opened rather
    /// than on every frame, because listing them touches the disk.
    manage_open: bool,
    manage: Installs,
    /// The copies ticked for deletion, by their `root`.
    checked: std::collections::HashSet<std::path::PathBuf>,
    /// The window was moved off the monitor. The process is still in the bar.
    concealed: bool,
    /// Takes the window off the screen, away from the window thread, so a show or hide
    /// cannot trip the
    /// "not responding" dialog.
    place: place::Place,
    /// Drop older trees after the install's follow-up check, so that check does not wipe the note.
    sweep_after_install: bool,
    /// Window artwork. Loaded once, then painted behind the controls.
    banner: Option<egui::TextureHandle>,
    /// Latest version already announced this session, so the hourly check does not repeat it.
    announced: Option<String>,
    /// An hourly check came in while a check or install was already running.
    check_when_idle: bool,
}

impl Launcher {
    fn new(waker: Arc<Waker>, shows: Receiver<()>) -> Self {
        let (tx, rx) = mpsc::channel();
        let found = home_dir().ok().and_then(|home| detect(&home));
        let installed_label = found.as_ref().map(|item| item.version.to_string());
        let headline = match &installed_label {
            Some(version) => format!("SolidCraft {version} is installed. Checking for updates…"),
            None => "Checking GitHub for SolidCraft…".to_string(),
        };
        let (settings, settings_note) = settings::Settings::load();
        let autostart_note = autostart::apply(&settings).err();
        let place = place::Place::start();
        let app = Self {
            waker,
            tx,
            rx,
            shows,
            tray: None,
            status: None,
            installed_label,
            installed: found,
            busy: true,
            headline,
            error: settings_note.or(autostart_note),
            progress: None,
            closed_to_tray: false,
            settings,
            options_open: false,
            manage_open: false,
            manage: Installs::default(),
            checked: std::collections::HashSet::new(),
            concealed: false,
            place,
            sweep_after_install: false,
            banner: None,
            announced: None,
            check_when_idle: false,
        };
        app.spawn_check();
        start_hourly_checks(app.tx.clone(), Arc::clone(&app.waker));
        app
    }

    fn spawn_check(&self) {
        let tx = self.tx.clone();
        let waker = Arc::clone(&self.waker);
        let preferred = self.settings.method.preferred();
        thread::spawn(move || {
            let event = match check_status(preferred) {
                Ok(status) => Event::Checked(status),
                Err(err) => Event::Failed(err),
            };
            let _ = tx.send(event);
            waker.wake();
        });
    }

    fn spawn_install(&self) {
        let tx = self.tx.clone();
        let waker = Arc::clone(&self.waker);
        let preferred = self.settings.method.preferred();
        thread::spawn(move || {
            let progress_tx = tx.clone();
            let progress_waker = Arc::clone(&waker);
            let event = match install_latest(preferred, move |received, total| {
                let _ = progress_tx.send(Event::Progress { received, total });
                progress_waker.wake();
            }) {
                Ok(installed) => Event::Installed(installed.version.to_string()),
                Err(err) => Event::Failed(err),
            };
            let _ = tx.send(event);
            waker.wake();
        });
    }

    /// Re-read the installed copies behind the Manage sheet.
    fn refresh_installs(&mut self) {
        self.manage = home_dir().ok().map(|home| list(&home)).unwrap_or_default();
        // A copy that has gone must not stay ticked, or the next Delete acts on nothing.
        let present: std::collections::HashSet<_> = self
            .manage
            .items
            .iter()
            .map(|item| item.root.clone())
            .collect();
        self.checked.retain(|root| present.contains(root));
    }

    /// Delete the ticked copies, off the window thread: a system package asks for a
    /// password, and that dialog must not freeze the window behind it.
    fn spawn_remove(&mut self) {
        let targets: Vec<Installed> = self
            .manage
            .items
            .iter()
            .filter(|item| self.checked.contains(&item.root))
            .cloned()
            .collect();
        if targets.is_empty() {
            return;
        }
        self.busy = true;
        self.error = None;
        self.headline = "Removing…".to_string();
        let tx = self.tx.clone();
        let waker = Arc::clone(&self.waker);
        thread::spawn(move || {
            let mut removed = Vec::new();
            let mut errors = Vec::new();
            match home_dir() {
                Ok(home) => {
                    for item in &targets {
                        match remove(&home, item) {
                            Ok(()) => removed.push(item.version.to_string()),
                            Err(err) => errors.push(err),
                        }
                    }
                }
                Err(err) => errors.push(err),
            }
            let _ = tx.send(Event::Removed { removed, errors });
            waker.wake();
        });
    }

    fn remember_local(&mut self) {
        let found = home_dir().ok().and_then(|home| detect(&home));
        self.installed_label = found.as_ref().map(|item| item.version.to_string());
        self.installed = found;
    }

    fn open_solidcraft(&mut self) {
        let Some(installed) = self.installed.clone() else {
            self.error = Some("SolidCraft is not installed".to_string());
            return;
        };
        match launch(&installed) {
            Ok(()) => {
                self.error = None;
                self.headline = "SolidCraft is opening.".to_string();
            }
            Err(err) => self.error = Some(err),
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Checked(status) => {
                    self.busy = false;
                    self.progress = None;
                    self.error = None;
                    self.headline = headline(&status);
                    self.installed_label = status.installed.clone();
                    if let Some((latest, installed)) =
                        announce_version(&status, self.announced.as_deref())
                    {
                        self.announced = Some(latest.clone());
                        notify::update_available(&latest, &installed);
                    }
                    self.status = Some(status);
                    self.remember_local();
                    if std::mem::take(&mut self.sweep_after_install) {
                        self.sweep_old(false);
                    }
                    self.follow_up_check();
                }
                Event::Removed { removed, errors } => {
                    self.busy = false;
                    self.headline = match removed.len() {
                        0 => "Nothing was removed.".to_string(),
                        1 => "Removed 1 copy of SolidCraft.".to_string(),
                        count => format!("Removed {count} copies of SolidCraft."),
                    };
                    self.error = (!errors.is_empty()).then(|| errors.join("\n"));
                    self.checked.clear();
                    self.refresh_installs();
                    self.remember_local();
                    // What is installed decides what an update downloads, so the status
                    // line is no longer true once a copy has gone.
                    self.spawn_check();
                }
                Event::Progress { received, total } => {
                    self.progress = Some((received, total));
                    self.headline = "Downloading SolidCraft…".to_string();
                }
                Event::Installed(version) => {
                    self.progress = None;
                    self.error = None;
                    self.installed_label = Some(version.clone());
                    self.headline = format!("SolidCraft {version} is installed.");
                    self.remember_local();
                    self.sweep_after_install = self.settings.remove_old_versions;
                    self.check_when_idle = false;
                    self.busy = true;
                    self.spawn_check();
                }
                Event::Failed(err) => {
                    self.busy = false;
                    self.progress = None;
                    self.error = Some(err);
                    self.follow_up_check();
                }
                Event::Hourly => self.request_automatic_check(),
            }
        }
        let update = self
            .status
            .as_ref()
            .is_some_and(|status| status.update_available);
        if let Some(icon) = &mut self.tray {
            icon.set_update(update);
        }
    }

    fn collect_requests(&mut self) -> Vec<Request> {
        let mut requests = Vec::new();
        while let Ok(()) = self.shows.try_recv() {
            requests.push(Request::Show);
        }
        let actions = self
            .tray
            .as_ref()
            .map(tray::Tray::actions)
            .unwrap_or_default();
        for action in actions {
            let request = match action {
                tray::TrayAction::Toggle => Request::Toggle,
                tray::TrayAction::Show => Request::Show,
                tray::TrayAction::Check => Request::Check,
                tray::TrayAction::Quit => Request::Quit,
            };
            if matches!(request, Request::Quit) {
                self.waker.quit_seen();
            }
            requests.push(request);
        }
        requests
    }

    fn store_settings(&mut self) -> bool {
        if let Err(err) = self.settings.save() {
            self.error = Some(err);
            return false;
        }
        if let Err(err) = autostart::apply(&self.settings) {
            self.error = Some(err);
            return false;
        }
        self.error = None;
        true
    }

    fn set_start_on_login(&mut self, on: bool) {
        let previous = self.settings.clone();
        self.settings.start_on_login = on;
        if !self.store_settings() {
            self.settings = previous;
        }
    }

    fn set_start_minimized(&mut self, on: bool) {
        let previous = self.settings.clone();
        self.settings.start_minimized = on;
        if !self.store_settings() {
            self.settings = previous;
        }
    }

    fn set_method(&mut self, method: settings::Method) {
        let previous = self.settings.clone();
        self.settings.method = method;
        if !self.store_settings() {
            self.settings = previous;
            return;
        }
        // The chosen format decides which asset the next check reports, so ask again.
        self.spawn_check();
    }

    fn set_remove_old_versions(&mut self, on: bool) {
        let previous = self.settings.clone();
        self.settings.remove_old_versions = on;
        if !self.store_settings() {
            self.settings = previous;
            return;
        }
        if on {
            self.sweep_old(true);
        }
    }

    /// `announce_empty` is for the checkbox. An install stays quiet when nothing was old.
    fn sweep_old(&mut self, announce_empty: bool) {
        let Ok(home) = home_dir() else {
            return;
        };
        let outcome = remove_old_copies(&home);
        if !outcome.errors.is_empty() {
            self.error = Some(outcome.errors.join(" "));
            return;
        }
        if outcome.removed.is_empty() {
            if announce_empty {
                self.headline = "No older SolidCraft copies were found.".to_string();
            }
            return;
        }
        let list = outcome.removed.join(", ");
        self.headline = if announce_empty {
            format!("Removed older copies: {list}.")
        } else {
            format!("{} Removed older copies: {list}.", self.headline)
        };
    }

    fn apply_check(&mut self, requests: &[Request]) {
        if self.busy
            || !requests
                .iter()
                .any(|request| matches!(request, Request::Check))
        {
            return;
        }
        self.error = None;
        self.headline = "Checking GitHub for SolidCraft…".to_string();
        self.spawn_check_now();
    }

    /// Hourly check. A click on Check again still shows "Checking…" in the window.
    fn request_automatic_check(&mut self) {
        if self.busy {
            self.check_when_idle = true;
            return;
        }
        self.spawn_check_now();
    }

    fn spawn_check_now(&mut self) {
        self.check_when_idle = false;
        self.busy = true;
        self.spawn_check();
    }

    fn follow_up_check(&mut self) {
        if self.check_when_idle && !self.busy {
            self.spawn_check_now();
        }
    }

    /// `true` when the window should open. `false` when the user quit.
    fn wait_in_tray(&mut self, wake_rx: &Receiver<()>) -> bool {
        loop {
            self.drain_events();
            let requests = self.collect_requests();
            self.apply_check(&requests);
            match tray_step(&requests) {
                TrayStep::Open => return true,
                TrayStep::Quit => return false,
                TrayStep::Stay => {}
            }
            if self.busy {
                let _ = wake_rx.recv_timeout(Duration::from_millis(200));
            } else if wake_rx.recv().is_err() {
                return false;
            }
        }
    }
}

struct Shell<'a> {
    launcher: &'a mut Launcher,
    quitting: bool,
}

impl Shell<'_> {
    fn handle(&mut self, ctx: &egui::Context, requests: &[Request]) {
        for request in requests {
            match request {
                Request::Check => {}
                Request::Show => self.reveal(ctx),
                Request::Toggle => {
                    if self.away_from_screen() {
                        self.reveal(ctx);
                    } else {
                        self.put_away(ctx);
                    }
                }
                Request::Quit => {
                    self.quitting = true;
                    self.launcher.closed_to_tray = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    ctx.request_repaint();
                }
            }
        }
    }

    fn away_from_screen(&self) -> bool {
        self.launcher.concealed
    }

    fn reveal(&mut self, ctx: &egui::Context) {
        self.launcher.concealed = false;
        self.launcher.place.reveal();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Take the window off the screen, leaving the process in the status bar.
    ///
    /// Hyprland moves the window itself; everywhere else the surface is hidden here. Doing
    /// both on Hyprland would unmap a window the compositor has already moved, and the
    /// event loop would then exit with it still on screen.
    fn conceal(&mut self, ctx: &egui::Context) {
        self.launcher.concealed = true;
        self.launcher.closed_to_tray = false;
        if self.launcher.place.hides_the_window() {
            self.launcher.place.conceal();
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }

    /// The × and the window-manager close both come here. The process stays in the bar.
    fn put_away(&mut self, ctx: &egui::Context) {
        if self.launcher.tray.is_some() {
            self.conceal(ctx);
            return;
        }
        self.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        // Close is noticed on the next frame. Ask for that frame, or an idle window never leaves.
        ctx.request_repaint();
    }
}

impl eframe::App for Shell<'_> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.launcher.drain_events();
        let requests = self.launcher.collect_requests();
        self.launcher.apply_check(&requests);
        self.handle(ctx, &requests);
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if close_requested && !self.quitting {
            // Closing the surface from here leaves it mapped. Cancel that and move it off screen.
            if self.launcher.tray.is_some() {
                self.conceal(ctx);
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if self.launcher.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if draw(ctx, self.launcher) {
            self.put_away(ctx);
        }
    }
}

/// Banner background. Same blue-black as the empty grid in `assets/window-banner.png`.
const WINDOW_FILL: egui::Color32 = egui::Color32::from_rgb(0x18, 0x1c, 0x24);

const BANNER_PNG: &[u8] = include_bytes!("../../../assets/window-banner.png");

/// Where the status line starts in the 600×400 window. The mark above it ends near y = 194.
const STATUS_TOP: f32 = 224.0;

const FOOTER: &str = "Download, install and open SolidCraft. New releases are checked against their published SHA-256.";

fn draw(ctx: &egui::Context, launcher: &mut Launcher) -> bool {
    let mut close = false;
    let mut panel = egui::Rect::NOTHING;
    let mut options_rect = egui::Rect::NOTHING;
    let mut options_clicked = false;
    let mut content_right = 0.0;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(WINDOW_FILL))
        .show(ctx, |ui| {
            // The picture is the window. A title bar above it would push the mark down.
            paint_banner(ui, &mut launcher.banner);
            panel = ui.max_rect();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new("SolidLauncher").size(15.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    let button = egui::Button::new(egui::RichText::new("×").size(16.0))
                        .min_size(egui::vec2(32.0, 24.0));
                    if ui
                        .add(button)
                        .on_hover_text("Close the window. SolidLauncher stays in the bar.")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
            let cursor = ui.cursor().min.y;
            let status_y = panel.min.y + STATUS_TOP;
            if status_y > cursor {
                ui.add_space(status_y - cursor);
            }
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(16, 0))
                .show(ui, |ui| {
                    ui.label(&launcher.headline);
                    ui.add_space(6.0);
                    ui.monospace(format!(
                        "Installed   {}",
                        launcher
                            .installed_label
                            .as_deref()
                            .unwrap_or("not installed")
                    ));
                    // Say where a copy came from when SolidLauncher did not put it there,
                    // so "Update" replacing it is never a surprise.
                    if let Some(note) = foreign_install_note(launcher.installed.as_ref()) {
                        ui.monospace(format!("            {note}"));
                    }
                    if let Some(status) = &launcher.status {
                        ui.monospace(format!("Latest      {}", status.latest));
                        ui.monospace(format!("Download    {}", status.asset));
                    }
                    if let Some((received, total)) = launcher.progress {
                        ui.add_space(12.0);
                        let fraction = if total == 0 {
                            0.0
                        } else {
                            received as f32 / total as f32
                        };
                        ui.add(egui::ProgressBar::new(fraction).show_percentage());
                    }
                    if let Some(error) = &launcher.error {
                        ui.add_space(12.0);
                        ui.colored_label(egui::Color32::from_rgb(0xff, 0x8a, 0x80), error);
                    }
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        let installed = launcher.installed.is_some();
                        let can_update = launcher
                            .status
                            .as_ref()
                            .is_some_and(|status| status.update_available);
                        if ui
                            .add_enabled(!launcher.busy && installed, egui::Button::new("Open"))
                            .clicked()
                        {
                            launcher.open_solidcraft();
                        }
                        let install_label = if installed { "Update" } else { "Install" };
                        if ui
                            .add_enabled(
                                !launcher.busy && (can_update || launcher.status.is_none()),
                                egui::Button::new(install_label),
                            )
                            .clicked()
                        {
                            launcher.busy = true;
                            launcher.error = None;
                            launcher.progress = None;
                            launcher.headline = "Downloading SolidCraft…".to_string();
                            launcher.spawn_install();
                        }
                        if ui
                            .add_enabled(!launcher.busy, egui::Button::new("Check again"))
                            .clicked()
                        {
                            launcher.busy = true;
                            launcher.error = None;
                            launcher.headline = "Checking GitHub for SolidCraft…".to_string();
                            launcher.spawn_check();
                        }
                        if ui
                            .add_enabled(
                                !launcher.busy,
                                egui::Button::new("Manage…").selected(launcher.manage_open),
                            )
                            .on_hover_text(
                                "See and remove the copies of SolidCraft on this computer",
                            )
                            .clicked()
                        {
                            launcher.manage_open = !launcher.manage_open;
                            if launcher.manage_open {
                                launcher.refresh_installs();
                            }
                        }
                        let options_button =
                            ui.add(egui::Button::new("Options").selected(launcher.options_open));
                        options_rect = options_button.rect;
                        options_clicked = options_button.clicked();
                        if options_clicked {
                            launcher.options_open = !launcher.options_open;
                        }
                    });
                    content_right = ui.min_rect().right();
                    // Keep the sentence on the bottom grid, under the buttons.
                    let spare = ui.available_height() - 36.0;
                    if spare > 0.0 {
                        ui.add_space(spare);
                    }
                    ui.label(FOOTER);
                });
        });
    if launcher.options_open
        && show_options(ctx, launcher, panel, options_rect, content_right)
        && !options_clicked
    {
        launcher.options_open = false;
    }
    if launcher.manage_open {
        show_manage(ctx, launcher);
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        launcher.options_open = false;
        launcher.manage_open = false;
    }
    close
}

/// The Manage copies sheet: everything installed, and a way to remove it.
///
/// A copy can be removed while it is the one in use. That is the point: without it
/// someone who installed the tarball cannot move to the AppImage until a new release
/// comes out, because an update keeps the format already there.
fn show_manage(ctx: &egui::Context, launcher: &mut Launcher) {
    let mut open = true;
    egui::Window::new("SolidCraft on this computer")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if launcher.manage.items.is_empty() {
                ui.label("No SolidCraft was found on this computer.");
                return;
            }
            let mut toggles: Vec<(std::path::PathBuf, bool)> = Vec::new();
            for item in &launcher.manage.items {
                ui.horizontal(|ui| {
                    let mut ticked = launcher.checked.contains(&item.root);
                    // A copy this launcher cannot remove is still listed, so the window
                    // tells the truth about the machine, but it cannot be ticked.
                    if ui
                        .add_enabled(
                            item.removable() && !launcher.busy,
                            egui::Checkbox::new(&mut ticked, item.label()),
                        )
                        .changed()
                    {
                        toggles.push((item.root.clone(), ticked));
                    }
                    if launcher.manage.is_active(item) {
                        ui.label(
                            egui::RichText::new("in use")
                                .small()
                                .color(egui::Color32::from_rgb(0x8a, 0xb4, 0xf8)),
                        );
                    }
                });
                ui.label(
                    egui::RichText::new(item.root.display().to_string())
                        .small()
                        .color(egui::Color32::from_gray(0x9a)),
                );
                if !item.removable() {
                    ui.label(
                        egui::RichText::new("Remove this one the way it was installed.")
                            .small()
                            .color(egui::Color32::from_gray(0x9a)),
                    );
                }
                ui.add_space(4.0);
            }
            for (root, ticked) in toggles {
                if ticked {
                    launcher.checked.insert(root);
                } else {
                    launcher.checked.remove(&root);
                }
            }
            ui.separator();
            let ticked = launcher.checked.len();
            // Removing the copy in use is allowed, but it should not be a surprise.
            let removing_active = launcher.manage.items.iter().any(|item| {
                launcher.checked.contains(&item.root) && launcher.manage.is_active(item)
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        ticked > 0 && !launcher.busy,
                        egui::Button::new(match ticked {
                            1 => "Delete 1 copy".to_string(),
                            count => format!("Delete {count} copies"),
                        }),
                    )
                    .clicked()
                {
                    launcher.spawn_remove();
                }
                if ui.button("Close").clicked() {
                    launcher.manage_open = false;
                }
            });
            if removing_active {
                ui.label(
                    egui::RichText::new(
                        "This includes the copy SolidCraft opens from. Install again \
                         afterwards to get it back.",
                    )
                    .small()
                    .color(egui::Color32::from_rgb(0xff, 0xc1, 0x07)),
                );
            }
            if launcher.manage.items.iter().any(|item| !item.removable()) {
                ui.label(
                    egui::RichText::new(
                        "A copy installed by a package manager or Flatpak is listed but \
                         not removed from here.",
                    )
                    .small()
                    .color(egui::Color32::from_gray(0x9a)),
                );
            }
        });
    if !open {
        launcher.manage_open = false;
    }
}

/// Options sit to the right of the status text, above the SHA-256 line.
/// Returns true when the click was outside the panel.
fn show_options(
    ctx: &egui::Context,
    launcher: &mut Launcher,
    panel: egui::Rect,
    options_rect: egui::Rect,
    content_right: f32,
) -> bool {
    let width = 216.0;
    let left = (content_right + 12.0)
        .max(options_rect.right() + 8.0)
        .clamp(
            panel.left() + 8.0,
            (panel.right() - width - 8.0).max(panel.left() + 8.0),
        );
    let bottom = options_rect
        .bottom()
        .clamp(panel.top() + 48.0, panel.bottom() - 44.0);
    let frame = egui::Frame::popup(ctx.style().as_ref())
        .fill(egui::Color32::from_rgb(0x2a, 0x30, 0x38))
        .inner_margin(egui::Margin::symmetric(10, 8));
    let response = egui::Area::new(egui::Id::new("solid-launcher-options"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(left, bottom))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .default_width(width + 20.0)
        .constrain_to(panel.shrink(4.0))
        .movable(false)
        .fade_in(false)
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_min_width(width);
                ui.set_max_width(width);
                let mut start_on_login = launcher.settings.start_on_login;
                if ui
                    .checkbox(&mut start_on_login, "Start on login")
                    .changed()
                {
                    launcher.set_start_on_login(start_on_login);
                }
                let mut start_minimized = launcher.settings.start_minimized;
                if ui
                    .checkbox(&mut start_minimized, "Start minimized")
                    .changed()
                {
                    launcher.set_start_minimized(start_minimized);
                }
                ui.label("Used the next time you log in.");
                // Where a platform replaces the installed copy in place there are no older
                // folders, so the option is left out rather than shown doing nothing.
                if keeps_old_versions() {
                    let mut remove_old = launcher.settings.remove_old_versions;
                    if ui
                        .checkbox(&mut remove_old, "Remove old versions")
                        .changed()
                    {
                        launcher.set_remove_old_versions(remove_old);
                    }
                    ui.label(
                        "Older SolidCraft folders in your home are removed. The one you are using stays.",
                    );
                }
                // Only shown where SolidCraft publishes more than one thing we can
                // install; elsewhere the single choice is not a choice.
                let choices = settings::Method::choices();
                if choices.len() > 1 {
                    ui.separator();
                    ui.label("Install SolidCraft as");
                    let chosen = launcher.settings.method;
                    for method in choices {
                        if ui
                            .radio(chosen == *method, method.label())
                            .on_hover_text(method.note())
                            .clicked()
                            && chosen != *method
                        {
                            launcher.set_method(*method);
                        }
                    }
                    ui.label(chosen.note());
                }
            });
        });
    response.response.clicked_elsewhere()
}

fn paint_banner(ui: &egui::Ui, slot: &mut Option<egui::TextureHandle>) {
    if slot.is_none() {
        if let Some(image) = banner_image() {
            *slot = Some(ui.ctx().load_texture(
                "window-banner",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }
    let Some(texture) = slot.as_ref() else {
        return;
    };
    let size = texture.size_vec2();
    if size.x < 1.0 || size.y < 1.0 {
        return;
    }
    let dest = egui::Rect::from_min_size(ui.max_rect().min, size);
    ui.painter().image(
        texture.id(),
        dest,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

fn banner_image() -> Option<egui::ColorImage> {
    let decoded = image::load_from_memory_with_format(BANNER_PNG, image::ImageFormat::Png).ok()?;
    let rgba = decoded.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

fn start_hourly_checks(tx: std::sync::mpsc::Sender<Event>, waker: Arc<Waker>) {
    let started = thread::Builder::new()
        .name("solid-launcher-hourly".to_string())
        .spawn(move || loop {
            thread::sleep(CHECK_EVERY);
            let _ = tx.send(Event::Hourly);
            waker.wake();
        });
    if started.is_err() {
        eprintln!("SolidLauncher: the hourly update check did not start");
    }
}

/// A newer release than the one installed, and not the version already announced this session.
/// Nothing installed is not an update, so that does not notify.
fn announce_version(status: &Status, already: Option<&str>) -> Option<(String, String)> {
    if !status.update_available {
        return None;
    }
    let installed = status.installed.clone()?;
    if already == Some(status.latest.as_str()) {
        return None;
    }
    Some((status.latest.clone(), installed))
}

/// How an installed SolidCraft got here, when it was not this launcher.
///
/// `None` for our own install: saying so on every line would be noise, and the common
/// case needs no explanation.
fn foreign_install_note(installed: Option<&Installed>) -> Option<&'static str> {
    let origin = installed?.origin;
    (!origin.is_ours()).then(|| origin.label())
}

fn headline(status: &Status) -> String {
    match (&status.installed, status.update_available) {
        (None, _) => format!("SolidCraft {} is ready to install.", status.latest),
        (Some(version), true) => format!(
            "SolidCraft {version} is installed. {latest} is available.",
            latest = status.latest
        ),
        (Some(version), false) => format!("SolidCraft {version} is up to date."),
    }
}

fn style(cc: &eframe::CreationContext<'_>) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = WINDOW_FILL;
    visuals.window_fill = WINDOW_FILL;
    visuals.extreme_bg_color = egui::Color32::from_rgb(0x14, 0x16, 0x1a);
    visuals.widgets.noninteractive.fg_stroke.color = egui::Color32::from_rgb(0xe6, 0xe6, 0xe6);
    cc.egui_ctx.set_visuals(visuals);
}

#[cfg(test)]
mod tests {
    use super::{announce_version, start_in_tray, tray_step, Request, Status, TrayStep};

    fn status(installed: Option<&str>, latest: &str, update: bool) -> Status {
        Status {
            installed: installed.map(str::to_string),
            latest: latest.to_string(),
            asset: String::new(),
            notes: String::new(),
            html_url: String::new(),
            kind: solid_launcher_native::PackageKind::TarGz,
            update_available: update,
        }
    }

    #[test]
    fn an_update_is_announced_once_for_each_version() {
        let found = status(Some("0.23.0"), "0.24.0", true);
        let note = announce_version(&found, None);
        assert!(note.is_some(), "{note:?}");
        let note = note.unwrap_or_default();
        assert_eq!(note.0, "0.24.0");
        assert_eq!(note.1, "0.23.0");
        assert!(announce_version(&found, Some("0.24.0")).is_none());
        assert!(announce_version(&status(Some("0.23.0"), "0.23.0", false), None).is_none());
        assert!(announce_version(&status(None, "0.24.0", true), None).is_none());
    }

    #[test]
    fn hidden_start_needs_a_tray_icon() {
        assert!(start_in_tray(true, true));
        assert!(!start_in_tray(true, false));
        assert!(!start_in_tray(false, true));
    }

    #[test]
    fn the_tray_opens_for_a_click_and_quits_from_the_menu() {
        assert_eq!(tray_step(&[]), TrayStep::Stay);
        assert_eq!(tray_step(&[Request::Check]), TrayStep::Stay);
        assert_eq!(tray_step(&[Request::Show]), TrayStep::Open);
        assert_eq!(tray_step(&[Request::Toggle]), TrayStep::Open);
        assert_eq!(tray_step(&[Request::Quit]), TrayStep::Quit);
        assert_eq!(tray_step(&[Request::Show, Request::Quit]), TrayStep::Quit);
    }
}
