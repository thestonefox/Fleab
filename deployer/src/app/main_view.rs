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
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                            egui::ComboBox::from_id_salt("game").width(ui.available_width()).truncate()
                                .selected_text(self.profile().map(|p| p.name.as_str()).unwrap_or("Select a game"))
                                .show_ui(ui, |ui| {
                                    for profile in &self.config.profiles {
                                        ui.selectable_value(&mut selected, Some(profile.id), &profile.name);
                                    }
                                });
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
                        ui.add_enabled_ui(self.worker.is_none() && !self.updates.busy() && !self.builds.is_empty(), |ui| {
                            egui::ComboBox::from_id_salt("build").width(ui.available_width()).truncate()
                                .selected_text(self.builds.get(self.selected_build).map(build_label)
                                    .unwrap_or_else(|| "No builds available".into()))
                                .show_ui(ui, |ui| {
                                    for (index, build) in self.builds.iter().enumerate() {
                                        ui.selectable_value(&mut self.selected_build, index, build_label(build))
                                            .on_hover_text(&build.filename);
                                    }
                                }).response.on_hover_text(self.builds.get(self.selected_build)
                                    .map(|b| b.filename.as_str()).unwrap_or("Refresh to look for published builds"));
                        });
                    });
                    ui.add_space(8.0);
                    let target = self.builds.get(self.selected_build).map(|b| b.target);
                    let state = target.and_then(|target| self.targets.get(&target));
                    design::card(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                            icons::paint(ui, Icon::Folder, rect.center(), 20.0, ui.visuals().weak_text_color());
                            ui.label(RichText::new("Installation").size(16.0).strong());
                        });
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
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if design::primary(ui, "Deploy", !busy && target.is_some() && !self.read_only && !running)
                            .on_hover_text(if running { "Close the running game before deploying" }
                                else { "Install the selected build" }).clicked() { self.deploy(ctx); }
                        if icons::text_button(ui, Icon::Play, "Launch", !busy && launch_enabled)
                            .on_hover_text(launch_reason.as_deref().unwrap_or("Start the installed build"))
                            .clicked() && let Some(target) = target { self.launch(ctx, target); }
                        if icons::button(ui, Icon::Refresh, "Refresh builds", !busy && self.profile().is_some() && !self.read_only)
                            .clicked() { self.refresh(ctx); }
                    });
                    if let Some(reason) = launch_reason { design::caption(ui, &reason); }
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
                        design::caption(ui, &format!("{} published builds available", self.builds.len()));
                    } else { design::caption(ui, &self.status); }
                    if self.read_only && let Some(path) = &self.config_path {
                        ui.small(format!("Configuration: {}", path.display()));
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_controls_fit_narrow_and_default_windows() {
        for size in [egui::vec2(480.0, 390.0), egui::vec2(780.0, 660.0)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
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
            let mut app = FleabApp {
                config,
                config_path: None,
                _config_lock: None,
                read_only: false,
                builds: vec![build],
                selected_build: 0,
                targets: BTreeMap::new(),
                runners: Vec::new(),
                status: "Ready".into(),
                error: false,
                worker: None,
                editor: None,
                children: Vec::new(),
                updates: update_view::Updates::default(),
            };
            let mut rectangles = Vec::new();
            for _ in 0..3 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| app.main_view(ctx),
                );
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
