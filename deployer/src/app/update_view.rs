use super::*;
use crate::updates::{self, Candidate, Client, Paths, Preferences};
use std::sync::atomic::AtomicU8;

enum Outcome {
    Check(Result<Option<Candidate>, String>),
    Install(Result<(), String>),
}
pub(super) struct Updates {
    paths: Option<Paths>,
    preferences: Preferences,
    preferences_error: Option<String>,
    candidate: Option<Candidate>,
    receiver: Option<mpsc::Receiver<Outcome>>,
    cancel: Arc<AtomicBool>,
    installing: Arc<AtomicBool>,
    progress: Arc<Mutex<f32>>,
    operation: Arc<AtomicU8>,
    pub message: String,
    error: bool,
    checking: bool,
    startup_frames: u8,
    automatic_attempted: bool,
    pub restarting: bool,
}
impl Default for Updates {
    fn default() -> Self {
        Self {
            paths: None,
            preferences: Preferences {
                automatic: false,
                ..Default::default()
            },
            preferences_error: None,
            candidate: None,
            receiver: None,
            cancel: Arc::new(AtomicBool::new(false)),
            installing: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(0.0)),
            operation: Arc::new(AtomicU8::new(0)),
            message: String::new(),
            error: false,
            checking: false,
            startup_frames: 0,
            automatic_attempted: false,
            restarting: false,
        }
    }
}
impl Updates {
    pub fn load() -> Self {
        let mut state = Self::default();
        match Paths::current() {
            Ok(paths) => {
                match Preferences::load(&paths.preferences) {
                    Ok(preferences) => state.preferences = preferences,
                    Err(error) => state.preferences_error = Some(format!("{error:#}")),
                }
                if let Some(report) = updates::install::last_report(&paths) {
                    state.message = report.message;
                }
                state.paths = Some(paths);
            }
            Err(error) => state.preferences_error = Some(format!("{error:#}")),
        }
        if let Some(error) = updates::install::startup_error() {
            state.preferences_error = Some(error.into());
        }
        state
    }
    pub fn busy(&self) -> bool {
        self.operation.load(Ordering::Relaxed) != 0 || updates::install::awaiting_health()
    }
    pub fn active_download(&self) -> bool {
        self.operation.load(Ordering::Relaxed) != 0
    }
    pub fn cancel(&self) {
        if !self.installing.load(Ordering::Relaxed) {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }
    fn save_preferences(&mut self) {
        if self.preferences_error.is_some() {
            return;
        }
        if let Some(paths) = &self.paths
            && let Err(error) = self.preferences.save(&paths.preferences)
        {
            self.preferences_error = Some(format!("Could not save update preferences: {error:#}"));
        }
    }
    pub fn tick(&mut self, ctx: &egui::Context, read_only: bool) {
        self.startup_frames = self.startup_frames.saturating_add(1);
        if updates::install::awaiting_health() && !read_only {
            if self.startup_frames >= 2 {
                if let Err(error) = updates::install::acknowledge_startup() {
                    self.message = format!("Update startup check failed: {error:#}");
                    self.error = true;
                }
            } else {
                ctx.request_repaint();
            }
        }
        if self.receiver.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
        if let Some(result) = self.receiver.as_ref().map(|r| r.try_recv()) {
            match result {
                Ok(outcome) => {
                    self.receiver = None;
                    self.operation.store(0, Ordering::Relaxed);
                    self.installing.store(false, Ordering::Relaxed);
                    self.checking = false;
                    match outcome {
                        Outcome::Check(Ok(candidate)) => {
                            self.candidate = candidate;
                            self.preferences.last_checked = updates::now();
                            self.save_preferences();
                            self.message = if let Some(candidate) = &self.candidate {
                                format!("Version {} is available", candidate.version)
                            } else {
                                "You're up to date.".into()
                            };
                            self.error = false;
                        }
                        Outcome::Check(Err(error)) | Outcome::Install(Err(error)) => {
                            self.message = error;
                            self.error = true;
                        }
                        Outcome::Install(Ok(())) => {
                            self.restarting = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.checking = false;
                    self.operation.store(0, Ordering::Relaxed);
                    self.message = "Update operation stopped. App files were preserved.".into();
                    self.error = true;
                }
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
        if !read_only
            && !self.restarting
            && !self.busy()
            && self.paths.is_some()
            && self.preferences_error.is_none()
            && self.preferences.automatic
        {
            ctx.request_repaint_after(std::time::Duration::from_secs(60));
            if self.startup_frames < 3 {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
            if self.startup_frames >= 3
                && self.receiver.is_none()
                && (!self.automatic_attempted || self.preferences.last_attempt != 0)
                && self.preferences.due(updates::now())
            {
                self.automatic_attempted = true;
                self.check(ctx);
            }
        }
    }
    fn check(&mut self, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        self.preferences.last_attempt = updates::now();
        self.save_preferences();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.checking = true;
        self.message = "Checking GitHub Releases…".into();
        self.error = false;
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = Client::new()
                .and_then(|client| client.check(updates::VERSION))
                .map_err(|e| {
                    if e.is::<ureq::Error>() {
                        "Couldn't check for updates. Check your internet connection and try again."
                            .into()
                    } else {
                        format!("Couldn't check for updates: {e:#}")
                    }
                });
            let _ = sender.send(Outcome::Check(result));
            ctx.request_repaint();
        });
    }
    fn download(&mut self, ctx: &egui::Context, config: &Config) {
        if self.receiver.is_some() || self.busy() {
            return;
        }
        let Some(candidate) = self.candidate.clone() else {
            return;
        };
        let Some(paths) = self.paths.clone() else {
            return;
        };
        self.cancel.store(false, Ordering::Relaxed);
        if let Ok(mut progress) = self.progress.lock() {
            *progress = 0.0;
        }
        self.operation.store(1, Ordering::Relaxed);
        self.error = false;
        self.message = "Downloading update…".into();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let cancel = self.cancel.clone();
        let installing = self.installing.clone();
        let progress = self.progress.clone();
        let operation = self.operation.clone();
        let ctx = ctx.clone();
        let config = config.clone();
        thread::spawn(move || {
            let result = (|| -> Result<()> {
                let client = Client::new()?;
                let download = client.download(&candidate, &paths, &cancel, |fraction| {
                    if let Ok(mut value) = progress.lock() {
                        *value = fraction;
                    }
                    ctx.request_repaint();
                })?;
                anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Update cancelled");
                operation.store(2, Ordering::Relaxed);
                ctx.request_repaint();
                let prepared = updates::install::prepare(
                    download,
                    &std::env::current_exe()?,
                    updates::install::ProcessIdentity::current()?,
                    &config,
                    &paths,
                )?;
                if cancel.load(Ordering::Relaxed) {
                    updates::install::discard(&prepared)?;
                    anyhow::bail!("Update cancelled");
                }
                installing.store(true, Ordering::Relaxed);
                operation.store(3, Ordering::Relaxed);
                ctx.request_repaint();
                if let Err(error) = updates::install::start(&prepared) {
                    let _ = updates::install::discard(&prepared);
                    return Err(error);
                }
                Ok(())
            })()
            .map_err(|e| format!("Update was not installed: {e:#}"));
            let _ = sender.send(Outcome::Install(result));
            ctx.request_repaint();
        });
    }
    fn progress(&self, ui: &mut egui::Ui) {
        let phase = self.operation.load(Ordering::Relaxed);
        if phase == 0 {
            return;
        }
        let fraction = self.progress.lock().map(|p| *p).unwrap_or_default();
        ui.add(
            egui::ProgressBar::new(fraction)
                .text(match phase {
                    1 => "Downloading update…",
                    2 => "Verifying and preparing update…",
                    _ => "Restarting to install…",
                })
                .animate(true),
        );
        if !self.installing.load(Ordering::Relaxed) && ui.button("Cancel update").clicked() {
            self.cancel();
        }
    }
    pub fn banner(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        allowed: bool,
        config: &Config,
    ) {
        if self.busy() {
            design::card(ui).show(ui, |ui| self.progress(ui));
            ui.add_space(8.0);
            return;
        }
        let Some(candidate) = self.candidate.clone() else {
            return;
        };
        design::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(format!("Version {} is available", candidate.version)).strong());
            if self.error {
                ui.colored_label(ui.visuals().error_fg_color, &self.message);
            }
            ui.horizontal_wrapped(|ui| {
                if candidate.envelope.is_some() {
                    if design::primary(ui, "Download and restart", allowed)
                        .on_hover_text("Install the signed update when no deployment is active")
                        .clicked()
                    {
                        self.download(ctx, config);
                    }
                } else {
                    ui.hyperlink_to("Download from GitHub", candidate.page());
                }
                ui.hyperlink_to("Release notes", candidate.page());
            });
        });
        ui.add_space(8.0);
    }
    pub fn settings(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        allowed: bool,
        config: &Config,
    ) {
        design::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Application updates").size(16.0).strong());
            design::caption(ui, &format!("Installed version {}", updates::VERSION));
            if ui.add_enabled(self.preferences_error.is_none(), egui::Checkbox::new(&mut self.preferences.automatic, "Check for updates automatically")).changed() { self.save_preferences(); }
            design::caption(ui, "Checks run in the background at most once a day. Updates install only when you choose.");
            if ui.add_enabled(self.receiver.is_none(), egui::Button::new("Check for updates")).clicked() { self.check(ctx); }
            if self.checking { ui.spinner(); }
            if self.error { ui.colored_label(ui.visuals().error_fg_color, &self.message); }
            else { design::caption(ui, &self.message); }
            if let Some(error) = &self.preferences_error { ui.colored_label(ui.visuals().error_fg_color, error); }
            if self.preferences.last_checked != 0
                && let Some(date) = chrono::DateTime::from_timestamp(self.preferences.last_checked as i64, 0) {
                    ui.small(format!("Last checked {} UTC", date.format("%d %b %Y, %H:%M")));
            }
            ui.hyperlink_to("View all releases", format!("{}/releases", updates::REPOSITORY));
        });
        ui.add_space(8.0);
        self.banner(ctx, ui, allowed, config);
        if let Some(candidate) = &self.candidate {
            if candidate.envelope.is_none() {
                design::caption(
                    ui,
                    "This release has no signed updater package. Download it from GitHub.",
                );
            }
            if !candidate.notes.is_empty() {
                design::card(ui).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label("Release notes");
                    ui.add(egui::Label::new(&candidate.notes).wrap());
                });
            }
        }
    }
}
