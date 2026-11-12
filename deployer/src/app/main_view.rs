use super::icons::Icon;
use super::*;

fn build_label(build: &Build) -> String {
    format!(
        "{} · {} · {}",
        build.version,
        build.target.label(),
        build.timestamp.format("%d %b %Y, %H:%M UTC")
    )
}

impl FleabApp {
    pub(super) fn open_settings(&mut self) {
        self.editor = Some(settings::Editor::new(
            self.profile().cloned().unwrap_or_default(),
            self.profile().is_some(),
        ));
    }

    pub(super) fn main_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill).inner_margin(24))
            .show(ctx, |ui| {
                layout::scroll(ui, "builds-scroll", |ui| {
                    ui.set_max_width(ui.available_width().min(860.0));
                    let busy = self.worker.is_some() || self.updates.busy();
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 48.0).max(200.0);
                        ui.allocate_ui_with_layout(egui::vec2(width, 66.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_min_width(width);
                            ui.heading("Deploy a build");
                            design::caption(ui, "Choose a game and a published build.");
                        });
                        if icons::button(ui, Icon::Settings, "Settings", !busy && !self.read_only).clicked() {
                            self.open_settings();
                        }
                    });
                    ui.add_space(12.0);
                    self.updates.banner(ctx, ui, !busy && !self.read_only, &self.config);
                    if self.config.profiles.is_empty() && !self.read_only {
                        design::card(ui).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add_space(24.0);
                            ui.heading("Add your first game");
                            design::caption(ui, "Create a profile to connect your published builds to this computer.");
                            ui.add_space(12.0);
                            if design::primary(ui, "Add game", !busy).clicked() { self.open_settings(); }
                            ui.add_space(24.0);
                        });
                        return;
                    }
                    let mut selected = self.config.selected_profile;
                    design::card(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label("Game");
                        ui.add_enabled_ui(!busy && !self.read_only, |ui| {
                            let picker = egui::ComboBox::from_id_salt("game").width(ui.available_width()).truncate()
                                .selected_text(self.profile().map(|p| p.name.as_str()).unwrap_or("Select a game"))
                                .show_ui(ui, |ui| {
                                    for profile in &self.config.profiles {
                                        if ui.selectable_value(&mut selected, Some(profile.id), &profile.name).clicked() { ui.close(); }
                                    }
                                    layout::list_navigation(ui);
                                });
                            controller::anchor(ui.ctx(), &picker.response);
                        });
                        if selected != self.config.selected_profile {
                            let mut config = self.config.clone();
                            config.selected_profile = selected;
                            match self.save(config) {
                                Ok(()) => { self.builds.clear(); self.targets.clear(); self.refresh(ctx); }
                                Err(error) => self.set_error(format!("{error:#}")),
                            }
                        }
                        ui.add_space(12.0);
                        ui.label("Published build");
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 36.0 - ui.spacing().item_spacing.x).max(100.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 36.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.add_enabled_ui(!busy && !self.builds.is_empty(), |ui| {
                                        egui::ComboBox::from_id_salt("build").width(width).truncate()
                                            .selected_text(self.builds.get(self.selected_build).map(build_label)
                                                .unwrap_or_else(|| "No builds available".into()))
                                            .show_ui(ui, |ui| {
                                                for (index, build) in self.builds.iter().enumerate() {
                                                    if ui.selectable_value(&mut self.selected_build, index, build_label(build))
                                                        .on_hover_text(&build.filename).clicked() { ui.close(); }
                                                }
                                                layout::list_navigation(ui);
                                            }).response.on_hover_text(self.builds.get(self.selected_build)
                                                .map(|b| b.filename.as_str()).unwrap_or("Refresh to look for published builds"));
                                    });
                                },
                            );
                            if icons::button(ui, Icon::Refresh, "Refresh builds and select newest",
                                !busy && self.profile().is_some() && !self.read_only).clicked() {
                                self.refresh(ctx);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let target = self.builds.get(self.selected_build).map(|b| b.target);
                    let state = target.and_then(|target| self.targets.get(&target));
                    design::card(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::CollapsingHeader::new(RichText::new("Installation").size(16.0).strong())
                            .id_salt(("installation", self.config.selected_profile, target.map(Target::label)))
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.add_space(4.0);
                                design::caption(ui, "Destination");
                                let destination = self.profile().and_then(|p| target.and_then(|t| p.destination(t)));
                                ui.add(egui::Label::new(destination.map(|p| p.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| "Choose a build to see its destination".into())).wrap());
                                ui.add_space(8.0);
                                design::caption(ui, "Installed build");
                                if let Some(record) = state.and_then(|s| s.installed.as_ref()) {
                                    let label = Build::parse(std::path::Path::new(&record.filename))
                                        .map(|build| build_label(&build)).unwrap_or_else(|_| record.filename.clone());
                                    ui.add(egui::Label::new(label).wrap()).on_hover_text(&record.filename);
                                } else if target.is_none() {
                                    let mut any = false;
                                    for (target, state) in &self.targets {
                                        if let Some(record) = &state.installed {
                                            let version = Build::parse(std::path::Path::new(&record.filename))
                                                .map(|build| build.version).unwrap_or_else(|_| record.filename.clone());
                                            ui.add(egui::Label::new(format!("{} · {}", target.label(), version)).wrap());
                                            any = true;
                                        }
                                    }
                                    if !any { ui.label("Not installed"); }
                                } else { ui.label("Not installed"); }
                            });
                        if let Some(error) = state.and_then(|s| s.error.as_ref()) {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });
                    let launch_reason = state.and_then(|s| s.launch_error.clone());
                    let launch_enabled = state.is_some_and(|s| s.installed.is_some()
                        && s.error.is_none() && s.launch_error.is_none());
                    let running = target.is_some_and(|target| self.children.iter().any(|(profile, t, _)| {
                        Some(*profile) == self.config.selected_profile && *t == target
                    }));
                    let busy = self.worker.is_some() || self.updates.busy();
                    let deploy_reason = if running {
                        Some("Close the running game before deploying".to_owned())
                    } else if target.is_none() {
                        Some("Choose a published build to deploy".to_owned())
                    } else { None };
                    let combined_reason = deploy_reason.clone().or_else(|| {
                        self.profile().zip(self.builds.get(self.selected_build)).and_then(|(profile, build)| {
                            launching::check_build(profile, build, std::env::consts::OS, &self.runners)
                                .err().map(|e| format!("{e:#}"))
                        })
                    });
                    let installed_reason = launch_reason.clone().or_else(|| {
                        (!launch_enabled).then(|| "No launchable build is installed for this target".to_owned())
                    });
                    let action_reason = match self.action {
                        Action::DeployAndLaunch => combined_reason,
                        Action::DeployOnly => deploy_reason,
                        Action::LaunchInstalled => installed_reason,
                    };
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let execute = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let execute = design::primary_button(ui,
                                egui::Button::new(self.action.label()).min_size(egui::vec2(176.0, 36.0))
                                    .corner_radius(egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 }),
                                !busy && !self.read_only && action_reason.is_none())
                                .on_hover_text(action_reason.as_deref().unwrap_or(match self.action {
                                    Action::DeployAndLaunch => "Install the selected build, then start it",
                                    Action::DeployOnly => "Install the selected build without starting it",
                                    Action::LaunchInstalled => "Start the installed build without deploying",
                                })).clicked();
                            let menu = design::primary_button(ui,
                                egui::Button::new("").min_size(egui::vec2(36.0, 36.0))
                                    .corner_radius(egui::CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }),
                                !busy && !self.read_only)
                                .on_hover_text("Choose action");
                            menu.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, menu.enabled(), "Choose action"));
                            let ink = if ui.visuals().dark_mode { egui::Color32::from_rgb(0, 51, 77) }
                                else { egui::Color32::WHITE };
                            icons::paint(ui, Icon::ChevronDown, menu.rect.center(), 16.0,
                                ink.gamma_multiply(if menu.enabled() { 1.0 } else { ui.visuals().disabled_alpha() }));
                            ui.painter().line_segment([
                                menu.rect.left_center() + egui::vec2(0.0, -9.0),
                                menu.rect.left_center() + egui::vec2(0.0, 9.0),
                            ], egui::Stroke::new(1.0_f32, ink.gamma_multiply(0.25)));
                            egui::Popup::menu(&menu).show(|ui| {
                                ui.set_min_width(200.0);
                                for action in Action::ALL {
                                    if ui.selectable_value(&mut self.action, action, action.label()).clicked() {
                                        ui.close();
                                    }
                                }
                            });
                            execute
                        }).inner;
                        if execute {
                            match self.action {
                                Action::DeployAndLaunch => self.deploy(ctx, true),
                                Action::DeployOnly => self.deploy(ctx, false),
                                Action::LaunchInstalled => if let Some(target) = target { self.launch(ctx, target); },
                            }
                        }
                    });
                    if let Some(reason) = action_reason { design::caption(ui, &reason); }
                    if let Some(worker) = &self.worker && let Ok(progress) = worker.progress.lock() {
                        ui.add_space(8.0);
                        let bar = egui::ProgressBar::new(progress.fraction.unwrap_or(0.0)).text(progress.phase);
                        ui.add(if progress.fraction.is_none() { bar.animate(true) } else { bar });
                        if progress.cancellable && ui.button("Cancel deployment").clicked() {
                            worker.cancel.store(true, Ordering::Relaxed);
                        }
                    }
                    ui.add_space(8.0);
                    if self.error { ui.colored_label(ui.visuals().error_fg_color, &self.status); }
                    else if !busy && self.status == "Ready" {
                        let count = self.builds.len();
                        let noun = if count == 1 { "build" } else { "builds" };
                        design::caption(ui, &format!("{count} published {noun} available"));
                    } else { design::caption(ui, &self.status); }
                    if self.read_only && let Some(path) = &self.config_path {
                        ui.small(format!("Configuration: {}", path.display()));
                    }
                    self.controller.hint(ui);
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_app() -> FleabApp {
        let profile = Profile {
            name: "A game with a very long profile name ".repeat(6),
            ..Default::default()
        };
        let config = Config {
            schema_version: 1,
            selected_profile: Some(profile.id),
            profiles: vec![profile],
        };
        let build = Build::parse(std::path::Path::new(
            "Game_1.0_20261108-095828_Windows64.tar.zst",
        ))
        .unwrap();
        FleabApp {
            config,
            config_path: None,
            _config_lock: None,
            read_only: false,
            builds: vec![build],
            selected_build: 0,
            action: Action::default(),
            targets: BTreeMap::new(),
            runners: Vec::new(),
            status: "Ready".into(),
            error: false,
            worker: None,
            editor: None,
            children: Vec::new(),
            updates: update_view::Updates::default(),
            tray: tray::State::default(),
            controller: controller::State::default(),
            activation: None,
        }
    }

    fn render(
        app: &mut FleabApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.main_view(ctx),
        )
    }

    fn click(app: &mut FleabApp, ctx: &egui::Context, position: egui::Pos2) {
        for pressed in [true, false] {
            render(
                app,
                ctx,
                egui::vec2(780.0, 660.0),
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn rescanning_preserves_the_deployment_and_launch_result() {
        let ctx = egui::Context::default();
        for scan_fails in [false, true] {
            let mut app = sample_app();
            let (sender, receiver) = mpsc::channel();
            sender
                .send(Outcome::Scan(ScanResult {
                    builds: if scan_fails {
                        Err("Source disconnected".into())
                    } else {
                        let mut builds = app.builds.clone();
                        builds.insert(
                            0,
                            Build::parse(std::path::Path::new(
                                "Game_2.0_20261112-090000_Windows64.tar.zst",
                            ))
                            .unwrap(),
                        );
                        Ok(builds)
                    },
                    targets: BTreeMap::new(),
                    runners: vec![],
                    feedback: Some((
                        "Build deployed, but could not launch: access denied".into(),
                        true,
                    )),
                    selection: BuildSelection::KeepCurrent,
                }))
                .unwrap();
            app.worker = Some(Worker {
                receiver,
                cancel: Arc::new(AtomicBool::new(false)),
                progress: Arc::new(Mutex::new(Progress {
                    phase: "Reading archive source",
                    fraction: None,
                    cancellable: false,
                })),
                deployment: false,
            });
            app.poll(&ctx);
            assert!(app.error);
            assert!(
                app.status
                    .starts_with("Build deployed, but could not launch: access denied")
            );
            assert_eq!(app.status.contains("Source disconnected"), scan_fails);
            assert_eq!(app.selected_build, usize::from(!scan_fails));
        }
    }

    #[test]
    fn explicit_refresh_selects_newest_and_failed_refresh_preserves_selection() {
        let ctx = egui::Context::default();
        let mut app = sample_app();
        let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let source = root.path().join("archives");
        fs::create_dir(&source).unwrap();
        let old = source.join("Game_1.0_20261108-095828_Windows64.tar.zst");
        let new = source.join("Game_2.0_20261112-090000_Windows64.tar.zst");
        fs::write(&old, []).unwrap();
        fs::write(&new, []).unwrap();
        app.builds = vec![Build::parse(&old).unwrap()];
        app.config.profiles[0].source = source.to_string_lossy().into_owned();
        app.config.profiles[0].destinations.insert(
            Target::WindowsClient,
            root.path().join("installed").to_string_lossy().into_owned(),
        );
        let wait = |app: &mut FleabApp| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while app.worker.is_some() {
                assert!(std::time::Instant::now() < deadline, "Refresh timed out");
                app.poll(&ctx);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        app.refresh(&ctx);
        wait(&mut app);
        assert_eq!(app.builds.len(), 2);
        assert_eq!(app.selected_build, 0);
        assert_eq!(app.builds[0].path, new);

        app.selected_build = 1;
        app.config.profiles[0].source = root
            .path()
            .join("disconnected")
            .to_string_lossy()
            .into_owned();
        app.refresh(&ctx);
        wait(&mut app);
        assert!(app.error);
        assert_eq!(app.builds.len(), 2);
        assert_eq!(app.selected_build, 1);
        assert_eq!(app.builds[1].path, old);

        let empty = root.path().join("empty");
        fs::create_dir(&empty).unwrap();
        app.config.profiles[0].source = empty.to_string_lossy().into_owned();
        app.refresh(&ctx);
        wait(&mut app);
        assert!(!app.error);
        assert!(app.builds.is_empty());
        assert_eq!(app.selected_build, 0);
    }

    #[test]
    fn installation_details_start_collapsed_and_can_be_expanded() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.0);
        let mut app = sample_app();
        let mut header = None;
        for _ in 0..3 {
            let output = render(&mut app, &ctx, egui::vec2(780.0, 660.0), vec![]);
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text() == "Destination")));
            header = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Installation" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
        }
        click(&mut app, &ctx, header.expect("Installation header"));
        let output = render(&mut app, &ctx, egui::vec2(780.0, 660.0), vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Destination")));
    }

    #[test]
    fn choosing_a_dropdown_action_does_not_execute_it() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut app = sample_app();
        assert!(app.action == Action::DeployAndLaunch);
        let mut button = None;
        for _ in 0..3 {
            let output = render(&mut app, &ctx, egui::vec2(780.0, 660.0), vec![]);
            button = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.width() - 176.0).abs() < 0.1
                        && (rect.rect.height() - 36.0).abs() < 0.1 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            });
        }
        let button = button.expect("Primary action button");
        click(
            &mut app,
            &ctx,
            button.right_center() + egui::vec2(18.0, 0.0),
        );
        let mut choice = None;
        for _ in 0..3 {
            let output = render(&mut app, &ctx, egui::vec2(780.0, 660.0), vec![]);
            choice = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Deploy only" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
        }
        click(&mut app, &ctx, choice.expect("Deploy only menu option"));
        assert!(app.action == Action::DeployOnly);
        assert!(app.worker.is_none(), "Selecting an action must not deploy");
        let output = render(&mut app, &ctx, egui::vec2(780.0, 660.0), vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Deploy only")));
    }

    #[test]
    fn build_controls_fit_narrow_and_default_windows() {
        for (size, action, theme) in [egui::vec2(480.0, 390.0), egui::vec2(780.0, 660.0)]
            .into_iter()
            .flat_map(|size| {
                Action::ALL.into_iter().flat_map(move |action| {
                    [egui::ThemePreference::Light, egui::ThemePreference::Dark]
                        .into_iter()
                        .map(move |theme| (size, action, theme))
                })
            })
        {
            let ctx = egui::Context::default();
            design::install(&ctx);
            ctx.set_theme(theme);
            let mut app = sample_app();
            app.action = action;
            let mut rectangles = Vec::new();
            for _ in 0..3 {
                let output = render(&mut app, &ctx, size, vec![]);
                rectangles.clear();
                for shape in output.shapes {
                    if let egui::Shape::Rect(rect) = shape.shape {
                        rectangles.push(rect.rect);
                    }
                }
            }
            assert!(rectangles.len() > 2, "Expected rendered build controls");
            assert!(
                rectangles
                    .iter()
                    .all(|r| r.left() >= -0.5 && r.right() <= size.x + 0.5),
                "Controls extend past {size:?}: {rectangles:?}"
            );
        }
    }
}
