use super::*;
use serde::{Deserialize, Serialize};
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Preferences {
    enabled: bool,
    minimise: bool,
    close: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: false,
            minimise: true,
            close: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Open,
    Settings,
    Exit,
    Game(uuid::Uuid, Target, Action),
}

#[derive(Clone, PartialEq, Eq)]
struct Model {
    games: Vec<(uuid::Uuid, String, Vec<Target>)>,
    blocked: bool,
    status: String,
}

struct Native {
    icon: TrayIcon,
    model: Option<Model>,
    commands: BTreeMap<String, Command>,
    receiver: mpsc::Receiver<String>,
    alive: Arc<AtomicBool>,
    actions: Vec<MenuItem>,
    status_item: Option<MenuItem>,
    settings_item: Option<MenuItem>,
}
impl Drop for Native {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

impl Native {
    fn new(ctx: &egui::Context) -> Result<Self> {
        let pixels =
            image::load_from_memory(include_bytes!("../../../assets/fleab.png"))?.into_rgba8();
        let (width, height) = pixels.dimensions();
        let icon = tray_icon::Icon::from_rgba(pixels.into_raw(), width, height)?;
        let icon = TrayIconBuilder::new()
            .with_id("fleab")
            .with_icon(icon)
            .with_tooltip("Fleab")
            .with_menu(Box::new(Menu::new()))
            .build()?;
        #[cfg(windows)]
        icon.set_tooltip(Some("Fleab"))?; // Detect failed Shell registration.
        let (sender, receiver) = mpsc::channel();
        // These handlers are process-global. Install them once and forward to
        // one persistent channel, even if the user toggles the tray off/on.
        event_bridge(ctx, sender);
        let alive = Arc::new(AtomicBool::new(true));
        let pulse = alive.clone();
        let context = ctx.clone();
        thread::spawn(move || {
            while pulse.load(Ordering::Relaxed) {
                thread::sleep(std::time::Duration::from_secs(1));
                if pulse.load(Ordering::Relaxed) {
                    wake(&context);
                }
            }
        });
        Ok(Self {
            icon,
            receiver,
            model: None,
            commands: BTreeMap::new(),
            alive,
            actions: Vec::new(),
            status_item: None,
            settings_item: None,
        })
    }

    fn update(&mut self, config: &Config, blocked: bool, status: &str) -> Result<()> {
        let model = Model {
            games: config
                .profiles
                .iter()
                .map(|profile| {
                    (
                        profile.id,
                        profile.name.clone(),
                        Target::ALL
                            .into_iter()
                            .filter(|t| profile.destination(*t).is_some())
                            .collect(),
                    )
                })
                .collect(),
            blocked,
            status: status.to_owned(),
        };
        if self.model.as_ref() == Some(&model) {
            return Ok(());
        }
        let status_text = status
            .chars()
            .take(100)
            .collect::<String>()
            .replace('&', "&&");
        if self
            .model
            .as_ref()
            .is_some_and(|old| old.games == model.games)
        {
            // Keep native menu handles alive while a popup is being tracked.
            // Replacing the menu on worker completion can destroy the menu the
            // user is currently interacting with, including its Exit command.
            for action in &self.actions {
                action.set_enabled(!blocked);
            }
            if let Some(item) = &self.status_item {
                item.set_text(&status_text);
            }
            if let Some(item) = &self.settings_item {
                item.set_enabled(!blocked);
            }
            self.model = Some(model);
            return Ok(());
        }
        let menu = Menu::new();
        let mut commands = BTreeMap::new();
        let mut action_items = Vec::new();
        let open = item(&mut commands, "open", "Open Fleab", true, Command::Open);
        menu.append(&open)?;
        menu.append(&PredefinedMenuItem::separator())?;
        for (id, name, targets) in &model.games {
            let game = Submenu::new(name.replace('&', "&&"), !targets.is_empty());
            for target in targets {
                let actions = Submenu::new(target.label(), true);
                for action in [
                    Action::DeployOnly,
                    Action::LaunchInstalled,
                    Action::DeployAndLaunch,
                ] {
                    let label = match action {
                        Action::DeployOnly => "Deploy latest",
                        Action::DeployAndLaunch => "Deploy and run latest",
                        Action::LaunchInstalled => "Run installed",
                    };
                    let key = format!("game:{id}:{}:{label}", target.label());
                    let action = item(
                        &mut commands,
                        &key,
                        label,
                        !blocked,
                        Command::Game(*id, *target, action),
                    );
                    action_items.push(action.clone());
                    if targets.len() == 1 {
                        game.append(&action)?;
                    } else {
                        actions.append(&action)?;
                    }
                }
                if targets.len() != 1 {
                    game.append(&actions)?;
                }
            }
            menu.append(&game)?;
        }
        menu.append(&PredefinedMenuItem::separator())?;
        let status_item = MenuItem::new(status_text, false, None);
        menu.append(&status_item)?;
        let settings = item(
            &mut commands,
            "settings",
            "Settings",
            !blocked,
            Command::Settings,
        );
        let exit = item(&mut commands, "exit", "Exit Fleab", true, Command::Exit);
        menu.append(&settings)?;
        menu.append(&exit)?;
        self.icon.set_menu(Some(Box::new(menu)));
        self.actions = action_items;
        self.status_item = Some(status_item);
        self.settings_item = Some(settings);
        self.commands = commands;
        self.model = Some(model);
        Ok(())
    }
}

fn item(
    commands: &mut BTreeMap<String, Command>,
    id: &str,
    label: &str,
    enabled: bool,
    command: Command,
) -> MenuItem {
    commands.insert(id.to_owned(), command);
    MenuItem::with_id(id, label, enabled, None)
}

// OnceLock handlers in tray-icon/muda cannot be replaced. Reuse a shared
// destination so toggling preferences does not leave events on a dead channel.
type Bridge = Mutex<Option<(egui::Context, mpsc::Sender<String>)>>;
#[cfg(windows)]
fn shell_available() -> bool {
    use windows_sys::{Win32::UI::WindowsAndMessaging::FindWindowW, core::w};
    !unsafe { FindWindowW(w!("Shell_TrayWnd"), std::ptr::null()) }.is_null()
}
static BRIDGE: std::sync::LazyLock<Bridge> = std::sync::LazyLock::new(|| Mutex::new(None));
static HANDLERS: std::sync::Once = std::sync::Once::new();
#[cfg(target_os = "linux")]
static HIDE_SUPPORTED: AtomicBool = AtomicBool::new(true);
#[cfg(windows)]
static WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(windows)]
static PAINT_PENDING: AtomicBool = AtomicBool::new(false);

pub(super) fn frame_started() {
    #[cfg(windows)]
    PAINT_PENDING.store(false, Ordering::Relaxed);
}

pub(super) fn attach(cc: &eframe::CreationContext<'_>) {
    #[cfg(target_os = "linux")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        HIDE_SUPPORTED.store(
            !matches!(
                cc.window_handle().map(|h| h.as_raw()),
                Ok(RawWindowHandle::Wayland(_))
            ),
            Ordering::Relaxed,
        );
    }
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = cc.window_handle()
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            WINDOW.store(handle.hwnd.get(), Ordering::Relaxed);
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = cc;
}

fn can_hide() -> bool {
    #[cfg(target_os = "linux")]
    {
        HIDE_SUPPORTED.load(Ordering::Relaxed)
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

pub(super) fn wake(ctx: &egui::Context) {
    ctx.request_repaint();
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IsWindowVisible, PostMessageW, WM_PAINT,
        };
        let window = WINDOW.load(Ordering::Relaxed);
        if window != 0
            && unsafe { IsWindowVisible(window as _) } == 0
            && !PAINT_PENDING.swap(true, Ordering::Relaxed)
        {
            // winit uses invalidation for request_redraw on Windows. Windows
            // does not deliver those paints to hidden windows; explicitly queue
            // a paint so the app can process tray commands and worker results.
            unsafe {
                PostMessageW(window as _, WM_PAINT, 0, 0);
            }
        }
    }
}
fn event_bridge(ctx: &egui::Context, sender: mpsc::Sender<String>) {
    *BRIDGE.lock().unwrap() = Some((ctx.clone(), sender));
    HANDLERS.call_once(|| {
        tray_icon::menu::MenuEvent::set_event_handler(Some(|event: tray_icon::menu::MenuEvent| {
            forward(event.id.0)
        }));
        tray_icon::TrayIconEvent::set_event_handler(Some(|event| {
            if matches!(event, tray_icon::TrayIconEvent::DoubleClick { .. }) {
                forward("open".into());
            }
        }));
    });
}
fn forward(id: String) {
    if let Some((ctx, sender)) = &*BRIDGE.lock().unwrap() {
        let _ = sender.send(id);
        wake(ctx);
    }
}

pub(super) fn latest_build(
    profile: &Profile,
    target: Target,
    launch_after: bool,
    runners: &[Runner],
) -> Result<Build> {
    archives::discover(profile)?.into_iter().find(|build| build.target == target
        && (!launch_after || launching::check_build(profile, build, std::env::consts::OS, runners).is_ok()))
        .context("No published build is available for this target and action. Check the source, platform and launch settings")
}

#[derive(Default)]
pub(super) struct State {
    preferences: Preferences,
    path: Option<PathBuf>,
    pub error: Option<String>,
    load_error: bool,
    native: Option<Native>,
    pub hidden: bool,
    pub quitting: bool,
    attempted: bool,
    #[cfg(target_os = "linux")]
    host: Option<bool>,
    #[cfg(target_os = "linux")]
    probe: Option<mpsc::Receiver<bool>>,
    #[cfg(target_os = "linux")]
    next_probe: Option<std::time::Instant>,
}

impl State {
    pub fn load(config: Option<&std::path::Path>) -> Self {
        let mut state = Self {
            path: config
                .and_then(|p| p.parent())
                .map(|p| p.join("desktop.json")),
            ..Default::default()
        };
        if let Some(path) = &state.path
            && path.exists()
        {
            match fs::read(path)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| Ok(serde_json::from_slice(&bytes)?))
            {
                Ok(preferences) => state.preferences = preferences,
                Err(error) => {
                    state.error = Some(format!("Could not load desktop settings: {error:#}"));
                    state.load_error = true;
                }
            }
        }
        state
    }

    pub fn available(&self) -> bool {
        self.native.is_some() && self.preferences.enabled && !self.quitting && {
            #[cfg(target_os = "linux")]
            {
                self.host == Some(true)
            }
            #[cfg(windows)]
            {
                shell_available()
            }
            #[cfg(not(any(target_os = "linux", windows)))]
            {
                true
            }
        }
    }
    pub fn close_to_tray(&self) -> bool {
        self.available() && can_hide() && self.preferences.close
    }
    pub fn minimise_to_tray(&self) -> bool {
        self.available() && can_hide() && self.preferences.minimise
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
    pub fn hide(&mut self, ctx: &egui::Context) {
        self.hidden = true;
        // Restore the minimized state before hiding, so event-loop wakeups can
        // still service tray commands and background work.
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        config: &Config,
        blocked: bool,
        status: &str,
    ) -> Vec<Command> {
        if !self.preferences.enabled {
            if self.hidden {
                self.show(ctx);
            }
            self.native = None;
            self.attempted = false;
            return Vec::new();
        }
        #[cfg(target_os = "linux")]
        self.probe_host(ctx);
        let supported = {
            #[cfg(target_os = "linux")]
            {
                self.host == Some(true)
            }
            #[cfg(windows)]
            {
                shell_available()
            }
            #[cfg(not(any(target_os = "linux", windows)))]
            {
                true
            }
        };
        if !supported && self.hidden {
            self.show(ctx);
        }
        if supported && self.native.is_none() && !self.attempted {
            self.attempted = true;
            match Native::new(ctx) {
                Ok(native) => {
                    self.native = Some(native);
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(format!(
                        "Tray unavailable; the window will stay accessible: {error:#}"
                    ))
                }
            }
        }
        let mut commands = Vec::new();
        if let Some(native) = &mut self.native {
            if let Err(error) = native.update(config, blocked, status) {
                self.error = Some(format!("Could not update tray menu: {error:#}"));
            }
            while let Ok(id) = native.receiver.try_recv() {
                if let Some(command) = native.commands.get(&id) {
                    commands.push(*command);
                }
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        commands
    }

    pub fn settings(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, read_only: bool) {
        let label = if cfg!(target_os = "macos") {
            "Keep Fleab in the menu bar"
        } else {
            "Keep Fleab in the system tray"
        };
        let mut changed = false;
        ui.add_enabled_ui(!read_only && !self.load_error, |ui| {
            changed |= ui.checkbox(&mut self.preferences.enabled, label).changed();
            ui.add_enabled_ui(self.preferences.enabled, |ui| {
                changed |= ui
                    .checkbox(
                        &mut self.preferences.minimise,
                        "Hide the window when minimised",
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.preferences.close,
                        "Keep running when the window is closed",
                    )
                    .changed();
            });
        });
        super::design::caption(
            ui,
            "Open Fleab, deploy or run games, and exit from the icon's menu. These settings apply immediately.",
        );
        if changed {
            let saved = self
                .path
                .as_ref()
                .context("Desktop settings path unavailable")
                .and_then(|path| crate::platform::atomic_json(path, &self.preferences));
            if let Err(error) = saved {
                self.error = Some(format!("Could not save desktop settings: {error:#}"));
            }
            self.attempted = false;
            ctx.request_repaint();
        }
        if self.preferences.enabled && !self.available() {
            super::design::caption(
                ui,
                "No usable tray icon is available yet. The window will minimise or close normally.",
            );
        }
        if self.preferences.enabled && !can_hide() {
            super::design::caption(
                ui,
                "This native Wayland session cannot hide or restore windows through the tray. Use XWayland/X11 for that behaviour; otherwise the window closes normally.",
            );
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }

    #[cfg(target_os = "linux")]
    fn probe_host(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.probe.as_ref().map(|receiver| receiver.try_recv()) {
            match result {
                Ok(available) => {
                    self.host = Some(available);
                    self.probe = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.host = Some(false);
                    self.probe = None;
                }
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
        if self.probe.is_none()
            && self
                .next_probe
                .is_none_or(|next| std::time::Instant::now() >= next)
        {
            let (sender, receiver) = mpsc::channel();
            let ctx = ctx.clone();
            thread::spawn(move || {
                let available = (|| -> Result<bool> {
                    let connection = zbus::blocking::Connection::session()?;
                    let watcher = zbus::blocking::Proxy::new(
                        &connection,
                        "org.kde.StatusNotifierWatcher",
                        "/StatusNotifierWatcher",
                        "org.kde.StatusNotifierWatcher",
                    )?;
                    Ok(watcher.get_property("IsStatusNotifierHostRegistered")?)
                })()
                .unwrap_or(false);
                let _ = sender.send(available);
                ctx.request_repaint();
            });
            self.probe = Some(receiver);
            self.next_probe = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_and_missing_or_corrupt_files_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("profiles.json");
        let mut state = State::load(Some(&config));
        assert!(!state.preferences.enabled);
        assert!(
            !state.close_to_tray(),
            "Unavailable tray must never hide a window"
        );
        state.preferences.enabled = true;
        state.preferences.minimise = false;
        crate::platform::atomic_json(state.path.as_ref().unwrap(), &state.preferences).unwrap();
        let state = State::load(Some(&config));
        assert!(state.preferences.enabled && state.preferences.close);
        assert!(!state.preferences.minimise);
        let path = root.path().join("desktop.json");
        fs::write(&path, b"invalid settings").unwrap();
        let state = State::load(Some(&config));
        assert!(state.load_error);
        assert!(!state.preferences.enabled);
        assert_eq!(fs::read(&path).unwrap(), b"invalid settings");
    }

    #[test]
    fn event_bridge_survives_disabling_and_reenabling_the_tray() {
        let ctx = egui::Context::default();
        let (old_sender, old_receiver) = mpsc::channel();
        event_bridge(&ctx, old_sender);
        forward("open".into());
        assert_eq!(old_receiver.try_recv().unwrap(), "open");
        let (new_sender, new_receiver) = mpsc::channel();
        event_bridge(&ctx, new_sender);
        forward("exit".into());
        assert_eq!(new_receiver.try_recv().unwrap(), "exit");
        assert!(old_receiver.try_recv().is_err());
    }

    #[test]
    fn latest_selection_filters_target_and_launch_compatibility() {
        let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let mut profile = Profile {
            source: root.path().to_string_lossy().into(),
            ..Default::default()
        };
        for target in [Target::WindowsClient, Target::LinuxClient] {
            profile.destinations.insert(
                target,
                root.path()
                    .join(format!("{target:?}"))
                    .to_string_lossy()
                    .into(),
            );
        }
        for name in [
            "Game_OLD_20261112-080000_Windows64.tar.zst",
            "Game_NEW_20261112-090000_Windows64.tar.zst",
            "Game_LINUX_20261112-100000_Linux64.tar.zst",
        ] {
            fs::write(root.path().join(name), []).unwrap();
        }
        assert_eq!(
            latest_build(&profile, Target::WindowsClient, false, &[])
                .unwrap()
                .version,
            "NEW"
        );
        if cfg!(target_os = "windows") {
            assert!(latest_build(&profile, Target::LinuxClient, true, &[]).is_err());
        }
        assert!(latest_build(&profile, Target::WindowsServer, false, &[]).is_err());
    }
}
