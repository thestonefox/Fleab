mod settings;
use crate::{
    archives::{self, Build, Target},
    deployment::{self, Installed, Progress},
    launching::{self, Runner},
    profiles::{self, Config, Profile},
};
use anyhow::{Context, Result};
use eframe::egui::{self, Color32, RichText};
use fs2::FileExt;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    path::PathBuf,
    process::Child,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

#[derive(Default)]
struct TargetState {
    installed: Option<Installed>,
    error: Option<String>,
    launch_error: Option<String>,
}
struct ScanResult {
    builds: Result<Vec<Build>, String>,
    targets: BTreeMap<Target, TargetState>,
    runners: Vec<Runner>,
}
enum Outcome {
    Scan(ScanResult),
    Deploy(Result<Installed, String>),
    Launch(Result<(Target, Child), String>),
}
struct Worker {
    receiver: mpsc::Receiver<Outcome>,
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    deployment: bool,
}

pub struct FleabApp {
    config: Config,
    config_path: Option<PathBuf>,
    _config_lock: Option<File>,
    read_only: bool,
    builds: Vec<Build>,
    selected_build: usize,
    targets: BTreeMap<Target, TargetState>,
    runners: Vec<Runner>,
    status: String,
    error: bool,
    worker: Option<Worker>,
    editor: Option<settings::Editor>,
    children: Vec<(Target, Child)>,
}

impl FleabApp {
    pub fn new(ctx: &egui::Context) -> Self {
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(16.0, 9.0);
        ctx.set_style(style);
        let mut app = Self {
            config: Config::default(),
            config_path: None,
            _config_lock: None,
            read_only: false,
            builds: Vec::new(),
            selected_build: 0,
            targets: BTreeMap::new(),
            runners: Vec::new(),
            status: "Create a profile in Settings to get started.".into(),
            error: false,
            worker: None,
            editor: None,
            children: Vec::new(),
        };
        let loaded = (|| -> Result<()> {
            let path = profiles::config_path()?;
            app.config_path = Some(path.clone());
            let parent = path.parent().context("Missing configuration parent")?;
            fs::create_dir_all(parent)?;
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(parent.join("fleab.lock"))?;
            lock.try_lock_exclusive()
                .context("Another Fleab instance is using these profiles; close it and restart")?;
            app._config_lock = Some(lock);
            app.config = Config::load(&path)?;
            if !app
                .config
                .profiles
                .iter()
                .any(|p| Some(p.id) == app.config.selected_profile)
            {
                app.config.selected_profile = app.config.profiles.first().map(|p| p.id);
            }
            Ok(())
        })();
        if let Err(error) = loaded {
            app.read_only = true;
            app.set_error(format!(
                "{error:#}. Configuration has been preserved. Correct the file and restart Fleab."
            ));
        } else {
            app.refresh(ctx);
        }
        app
    }
    fn profile(&self) -> Option<&Profile> {
        self.config
            .profiles
            .iter()
            .find(|p| Some(p.id) == self.config.selected_profile)
    }
    fn set_error(&mut self, text: String) {
        self.status = text;
        self.error = true;
    }
    fn save(&mut self, config: Config) -> Result<()> {
        anyhow::ensure!(
            !self.read_only,
            "Configuration is read-only until its load error is resolved"
        );
        config.save(
            self.config_path
                .as_deref()
                .context("No configuration path")?,
        )?;
        self.config = config;
        Ok(())
    }
    fn start(
        &mut self,
        ctx: &egui::Context,
        phase: &'static str,
        deployment: bool,
        task: impl FnOnce(Arc<AtomicBool>, Arc<Mutex<Progress>>) -> Outcome + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(Progress {
            phase,
            fraction: None,
            cancellable: false,
        }));
        let thread_cancel = cancel.clone();
        let thread_progress = progress.clone();
        let context = ctx.clone();
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                task(thread_cancel, thread_progress)
            }));
            if let Ok(outcome) = result {
                let _ = sender.send(outcome);
            }
            context.request_repaint();
        });
        self.worker = Some(Worker {
            receiver,
            cancel,
            progress,
            deployment,
        });
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        if self.worker.is_some() || self.read_only {
            return;
        }
        let Some(profile) = self.profile().cloned() else {
            self.builds.clear();
            self.targets.clear();
            return;
        };
        self.status = "Reading archive source…".into();
        self.error = false;
        self.start(ctx, "Reading archive source", false, move |_, _| {
            let runners = if cfg!(target_os = "linux") {
                launching::discover_runners()
            } else {
                Vec::new()
            };
            let mut targets = BTreeMap::new();
            for target in Target::ALL {
                if profile.destination(target).is_none() {
                    continue;
                }
                let mut state = TargetState::default();
                match deployment::installed(&profile, target) {
                    Ok(record) => {
                        state.installed = record;
                        if state.installed.is_some() {
                            state.launch_error =
                                launching::plan(&profile, target, std::env::consts::OS, &runners)
                                    .err()
                                    .map(|e| format!("{e:#}"));
                        }
                    }
                    Err(e) => state.error = Some(format!("{e:#}")),
                }
                targets.insert(target, state);
            }
            Outcome::Scan(ScanResult {
                builds: archives::discover(&profile).map_err(|e| format!("{e:#}")),
                targets,
                runners,
            })
        });
    }
    fn poll(&mut self, ctx: &egui::Context) {
        self.children
            .retain_mut(|(_, child)| matches!(child.try_wait(), Ok(None)));
        let outcome = self
            .worker
            .as_ref()
            .map(|worker| worker.receiver.try_recv());
        match outcome {
            Some(Ok(outcome)) => {
                self.worker = None;
                match outcome {
                    Outcome::Scan(result) => {
                        self.targets = result.targets;
                        self.runners = result.runners;
                        match result.builds {
                            Ok(builds) => {
                                let previous = self
                                    .builds
                                    .get(self.selected_build)
                                    .map(|b| b.filename.clone());
                                self.builds = builds;
                                self.selected_build = previous
                                    .and_then(|p| self.builds.iter().position(|b| b.filename == p))
                                    .unwrap_or(0);
                                self.status = if self.builds.is_empty() {
                                    "No matching archives. Configure a destination to enable that target, then Refresh.".into()
                                } else {
                                    "Ready".into()
                                };
                                self.error = false;
                            }
                            Err(error) => {
                                self.builds.clear();
                                self.set_error(error);
                            }
                        }
                    }
                    Outcome::Deploy(result) => match result {
                        Ok(record) => {
                            self.refresh(ctx);
                            self.status = format!("Installed {}", record.filename);
                            self.error = false;
                        }
                        Err(error) => self.set_error(error),
                    },
                    Outcome::Launch(result) => match result {
                        Ok((target, child)) => {
                            self.children.push((target, child));
                            self.status = "Started the installed build.".into();
                            self.error = false;
                        }
                        Err(error) => self.set_error(error),
                    },
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.worker = None;
                self.set_error(
                    "Operation stopped unexpectedly. Retry to recover any interrupted deployment."
                        .into(),
                );
            }
            _ => (),
        }
    }
    fn deploy(&mut self, ctx: &egui::Context) {
        let Some(profile) = self.profile().cloned() else {
            return;
        };
        let Some(build) = self.builds.get(self.selected_build).cloned() else {
            return;
        };
        let config = self.config.clone();
        let repaint = ctx.clone();
        self.status = "Deploying…".into();
        self.error = false;
        self.start(
            ctx,
            "Preparing deployment",
            true,
            move |cancel, progress| {
                Outcome::Deploy(
                    deployment::deploy(&config, &profile, &build, cancel, |p| {
                        if let Ok(mut current) = progress.lock() {
                            *current = p;
                        }
                        repaint.request_repaint();
                    })
                    .map_err(|e| format!("{e:#}")),
                )
            },
        );
    }
    fn launch(&mut self, ctx: &egui::Context, target: Target) {
        let Some(profile) = self.profile().cloned() else {
            return;
        };
        let runners = self.runners.clone();
        self.start(ctx, "Launching installed build", false, move |_, _| {
            Outcome::Launch(
                launching::launch(&profile, target, &runners)
                    .map(|child| (target, child))
                    .map_err(|e| format!("{e:#}")),
            )
        });
    }
}

impl eframe::App for FleabApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll(ctx);
        if let Some(worker) = &self.worker {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
            if worker.deployment && ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                worker.cancel.store(true, Ordering::Relaxed);
            }
        }
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Fleab").color(Color32::from_rgb(223, 175, 55)));
                ui.label("Build it. Hop it. Play it.");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            self.worker.is_none() && !self.read_only,
                            egui::Button::new("Settings"),
                        )
                        .clicked()
                    {
                        self.editor = Some(settings::Editor::new(
                            self.profile().cloned().unwrap_or_default(),
                            self.profile().is_some(),
                        ));
                    }
                });
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            let busy = self.worker.is_some();
            let mut selected = self.config.selected_profile;
            ui.label(RichText::new("Game").strong());
            ui.add_enabled_ui(!busy && self.editor.is_none() && !self.read_only, |ui| {
                egui::ComboBox::from_id_salt("game")
                    .width(ui.available_width())
                    .selected_text(
                        self.profile()
                            .map(|p| p.name.as_str())
                            .unwrap_or("Create a profile in Settings"),
                    )
                    .show_ui(ui, |ui| {
                        for p in &self.config.profiles {
                            ui.selectable_value(&mut selected, Some(p.id), &p.name);
                        }
                    });
            });
            if selected != self.config.selected_profile {
                let mut config = self.config.clone();
                config.selected_profile = selected;
                match self.save(config) {
                    Ok(()) => {
                        self.builds.clear();
                        self.targets.clear();
                        self.refresh(ctx);
                    }
                    Err(e) => self.set_error(format!("{e:#}")),
                }
            }
            ui.label(RichText::new("Build").strong());
            ui.add_enabled_ui(!busy && !self.builds.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("build")
                    .width(ui.available_width())
                    .selected_text(
                        self.builds
                            .get(self.selected_build)
                            .map(|b| b.filename.as_str())
                            .unwrap_or("No matching builds"),
                    )
                    .show_ui(ui, |ui| {
                        for (i, b) in self.builds.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_build, i, &b.filename);
                        }
                    });
            });
            let target = self.builds.get(self.selected_build).map(|b| b.target);
            ui.label(RichText::new("Destination").strong());
            ui.label(
                self.profile()
                    .and_then(|p| target.and_then(|t| p.destination(t)))
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "—".into()),
            );
            ui.label(RichText::new("Currently installed").strong());
            let state = target.and_then(|t| self.targets.get(&t));
            ui.label(
                state
                    .and_then(|s| s.installed.as_ref())
                    .map(|r| r.filename.as_str())
                    .unwrap_or("No recorded installation"),
            );
            if let Some(error) = state.and_then(|s| s.error.as_ref()) {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            let launch_reason = state.and_then(|s| s.launch_error.clone());
            let launch_enabled = state.is_some_and(|s| {
                s.installed.is_some() && s.error.is_none() && s.launch_error.is_none()
            });
            let running = target.is_some_and(|t| self.children.iter().any(|(ct, _)| *ct == t));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !busy && target.is_some() && !self.read_only && !running,
                        egui::Button::new("Deploy"),
                    )
                    .on_hover_text(if running {
                        "Close the running game before deploying"
                    } else {
                        "Install the selected archive"
                    })
                    .clicked()
                {
                    self.deploy(ctx);
                }
                if ui
                    .add_enabled(!busy && launch_enabled, egui::Button::new("Launch"))
                    .on_hover_text(
                        launch_reason
                            .as_deref()
                            .unwrap_or("Start the currently installed build"),
                    )
                    .clicked()
                    && let Some(target) = target
                {
                    self.launch(ctx, target);
                }
                if ui
                    .add_enabled(
                        !busy && self.profile().is_some() && !self.read_only,
                        egui::Button::new("Refresh"),
                    )
                    .clicked()
                {
                    self.refresh(ctx);
                }
            });
            if let Some(reason) = launch_reason {
                ui.small(reason);
            }
            if let Some(worker) = &self.worker
                && let Ok(progress) = worker.progress.lock()
            {
                let bar =
                    egui::ProgressBar::new(progress.fraction.unwrap_or(0.0)).text(progress.phase);
                ui.add(if progress.fraction.is_none() {
                    bar.animate(true)
                } else {
                    bar
                });
                if progress.cancellable && ui.button("Cancel").clicked() {
                    worker.cancel.store(true, Ordering::Relaxed);
                }
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(110.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(&self.status).color(if self.error {
                        Color32::LIGHT_RED
                    } else {
                        ui.visuals().text_color()
                    }));
                });
            if self.read_only
                && let Some(path) = &self.config_path
            {
                ui.small(format!("Configuration: {}", path.display()));
            }
        });
        self.settings(ctx);
    }
}
