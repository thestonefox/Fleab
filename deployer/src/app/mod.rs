mod design;
mod icons;
#[cfg(target_os = "linux")]
mod linux_theme;
mod main_view;
mod settings;
mod update_view;
use crate::{
    archives::{self, Build, Target},
    deployment::{self, Installed, Progress},
    launching::{self, Runner},
    profiles::{self, Config, Profile},
};
use anyhow::{Context, Result};
use eframe::egui::{self, RichText};
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
    Launch(Result<(uuid::Uuid, Target, Child), String>),
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
    children: Vec<(uuid::Uuid, Target, Child)>,
    updates: update_view::Updates,
}

impl FleabApp {
    pub fn new(ctx: &egui::Context) -> Self {
        design::install(ctx);
        #[cfg(target_os = "linux")]
        linux_theme::watch(ctx);
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
            updates: update_view::Updates::default(),
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
        app.updates = update_view::Updates::load();
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
        if self.worker.is_some() || self.read_only || self.updates.active_download() {
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
            .retain_mut(|(_, _, child)| matches!(child.try_wait(), Ok(None)));
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
                            Err(error) => self.set_error(error),
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
                        Ok((profile, target, child)) => {
                            self.children.push((profile, target, child));
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
        if self.updates.busy() {
            return;
        }
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
        if self.updates.busy() {
            return;
        }
        let Some(profile) = self.profile().cloned() else {
            return;
        };
        let runners = self.runners.clone();
        self.start(ctx, "Launching installed build", false, move |_, _| {
            Outcome::Launch(
                launching::launch(&profile, target, &runners)
                    .map(|child| (profile.id, target, child))
                    .map_err(|e| format!("{e:#}")),
            )
        });
    }
}

impl eframe::App for FleabApp {
    #[cfg(target_os = "linux")]
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        linux_theme::system_input(ctx, input);
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll(ctx);
        self.updates.tick(ctx, self.read_only);
        if self.updates.active_download()
            && !self.updates.restarting
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.updates.cancel();
        }
        if let Some(worker) = &self.worker {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
            if worker.deployment && ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                worker.cancel.store(true, Ordering::Relaxed);
            }
        }
        if self.editor.is_some() {
            self.settings(ctx);
        } else {
            self.main_view(ctx);
        }
    }
}
