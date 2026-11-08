use super::*;
use crate::profiles::RunnerKind;

pub(super) struct Editor {
    profile: Profile,
    existing: bool,
    game_args: String,
    runner_args: String,
    error: String,
}

impl Editor {
    pub fn new(profile: Profile, existing: bool) -> Self {
        Self {
            game_args: profile.launch.game_arguments.join("\n"),
            runner_args: profile.launch.runner_arguments.join("\n"),
            profile,
            existing,
            error: String::new(),
        }
    }
}
fn folder(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width((ui.available_width() - 95.0).max(100.0)),
        );
        if ui.button("Browse…").clicked()
            && let Some(path) = rfd::FileDialog::new().set_title(label).pick_folder()
        {
            *value = path.to_string_lossy().into();
        }
    });
}
impl FleabApp {
    pub(super) fn settings(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        let mut open = true;
        let mut action = None;
        let size = egui::vec2(
            (ctx.content_rect().width() - 28.0).min(620.0),
            ctx.content_rect().height() - 36.0,
        );
        egui::Window::new("Profiles and launch settings").open(&mut open).fixed_size(size).constrain_to(ctx.content_rect()).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New profile").clicked() { action = Some("new"); }
                if ui.button("Import JSON…").clicked() { action = Some("import"); }
                if ui.button("Export JSON…").clicked() { action = Some("export"); }
            });
            egui::ComboBox::from_id_salt("edit-profile").selected_text(&editor.profile.name).show_ui(ui, |ui| {
                for profile in &self.config.profiles {
                    if ui.selectable_label(profile.id == editor.profile.id, &profile.name).clicked() { editor = Editor::new(profile.clone(), true); }
                }
            });
            egui::ScrollArea::vertical().max_height((size.y - 190.0).max(100.0)).show(ui, |ui| {
                ui.label("Profile name"); ui.text_edit_singleline(&mut editor.profile.name);
                folder(ui, "Archive source directory", &mut editor.profile.source);
                ui.separator(); ui.small("Configure a destination to show that target's builds. Blank destinations stay disabled.");
                for target in Target::ALL { folder(ui, target.label(), editor.profile.destinations.entry(target).or_default()); }
                egui::CollapsingHeader::new("Executable overrides and launch arguments").show(ui, |ui| {
                    ui.small("Optional paths relative to each deployment directory. Leave empty for automatic detection.");
                    for target in Target::ALL { ui.label(target.label()); ui.text_edit_singleline(editor.profile.executable_overrides.entry(target).or_default()); }
                    ui.label("Game/server arguments — one argument per line");
                    ui.add(egui::TextEdit::multiline(&mut editor.game_args).desired_rows(3).desired_width(f32::INFINITY));
                    ui.small("Spaces stay inside an argument. Do not add shell quotes.");
                });
                egui::CollapsingHeader::new("Windows builds on Linux (Wine / Proton)").show(ui, |ui| {
                    let label = if editor.profile.launch.runner.is_empty() { "Automatic discovery".into() } else { editor.profile.launch.runner.clone() };
                    egui::ComboBox::from_id_salt("runner").selected_text(label).show_ui(ui, |ui| {
                        if ui.selectable_label(editor.profile.launch.runner.is_empty(), "Automatic discovery").clicked() { editor.profile.launch.runner.clear(); editor.profile.launch.runner_kind = RunnerKind::Auto; }
                        for runner in &self.runners {
                            if ui.selectable_label(editor.profile.launch.runner == runner.path.to_string_lossy(), &runner.name).clicked() {
                                editor.profile.launch.runner = runner.path.to_string_lossy().into(); editor.profile.launch.runner_kind = runner.kind; editor.profile.launch.steam_root = runner.steam_root.to_string_lossy().into();
                            }
                        }
                    });
                    egui::ComboBox::from_id_salt("runner-kind").selected_text(format!("{:?}", editor.profile.launch.runner_kind)).show_ui(ui, |ui| {
                        for kind in [RunnerKind::Auto, RunnerKind::Wine, RunnerKind::Proton] { ui.selectable_value(&mut editor.profile.launch.runner_kind, kind, format!("{kind:?}")); }
                    });
                    ui.label("Manual runner executable (optional)");
                    ui.horizontal(|ui| { ui.text_edit_singleline(&mut editor.profile.launch.runner); if ui.button("Browse…").clicked() && let Some(path) = rfd::FileDialog::new().pick_file() { editor.profile.launch.runner = path.to_string_lossy().into(); } });
                    folder(ui, "Compatibility prefix / Proton data directory (optional)", &mut editor.profile.launch.prefix);
                    folder(ui, "Steam root directory (needed for manual Proton)", &mut editor.profile.launch.steam_root);
                    ui.label("Runner argument override — one per line (Proton defaults to run)");
                    ui.add(egui::TextEdit::multiline(&mut editor.runner_args).desired_rows(2).desired_width(f32::INFINITY));
                });
            });
            if !editor.error.is_empty() { ui.colored_label(Color32::LIGHT_RED, &editor.error); }
            ui.horizontal(|ui| {
                if ui.button("Save profile").clicked() { action = Some("save"); }
                if ui.add_enabled(editor.existing, egui::Button::new("Delete profile")).on_hover_text("Removes configuration only; installed files are kept").clicked() { action = Some("delete"); }
                if ui.button("Cancel").clicked() { action = Some("cancel"); }
            });
        });
        editor.profile.launch.game_arguments =
            editor.game_args.lines().map(str::to_owned).collect();
        editor.profile.launch.runner_arguments =
            editor.runner_args.lines().map(str::to_owned).collect();
        let result = (|| -> Result<()> {
            match action {
                Some("cancel") => open = false,
                Some("new") => editor = Editor::new(Profile::default(), false),
                Some("import") => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Fleab profile", &["json"])
                        .pick_file()
                    {
                        editor = Editor::new(Config::import_profile(&path)?, false);
                    }
                }
                Some("export") => {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("fleab-profile.json")
                        .add_filter("JSON", &["json"])
                        .save_file()
                    {
                        Config::export_profile(&editor.profile, &path)?;
                    }
                }
                Some("save") => {
                    let mut config = self.config.clone();
                    if let Some(p) = config
                        .profiles
                        .iter_mut()
                        .find(|p| p.id == editor.profile.id)
                    {
                        *p = editor.profile.clone();
                    } else {
                        config.profiles.push(editor.profile.clone());
                    }
                    config.selected_profile = Some(editor.profile.id);
                    config.validate_paths()?;
                    self.save(config)?;
                    self.builds.clear();
                    self.targets.clear();
                    self.refresh(ctx);
                    open = false;
                }
                Some("delete") => {
                    let mut config = self.config.clone();
                    config.profiles.retain(|p| p.id != editor.profile.id);
                    if config.selected_profile == Some(editor.profile.id) {
                        config.selected_profile = config.profiles.first().map(|p| p.id);
                    }
                    self.save(config)?;
                    self.builds.clear();
                    self.targets.clear();
                    self.refresh(ctx);
                    open = false;
                }
                _ => (),
            }
            Ok(())
        })();
        if let Err(error) = result {
            editor.error = format!("{error:#}");
        }
        if open {
            self.editor = Some(editor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text_bounds(shape: &egui::epaint::Shape, found: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == "Save profile" => {
                found.push(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    text_bounds(shape, found);
                }
            }
            _ => (),
        }
    }
    #[test]
    fn settings_save_stays_inside_small_windows() {
        for size in [egui::vec2(700.0, 490.0), egui::vec2(480.0, 390.0)] {
            let ctx = egui::Context::default();
            let mut app = FleabApp {
                config: Config::default(),
                config_path: None,
                _config_lock: None,
                read_only: false,
                builds: Vec::new(),
                selected_build: 0,
                targets: BTreeMap::new(),
                runners: Vec::new(),
                status: String::new(),
                error: false,
                worker: None,
                editor: Some(Editor::new(Profile::default(), false)),
                children: Vec::new(),
            };
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            // egui windows settle their auto-size on a second layout pass.
            let mut bounds = Vec::new();
            for _ in 0..3 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| app.settings(ctx),
                );
                bounds.clear();
                for shape in &output.shapes {
                    text_bounds(&shape.shape, &mut bounds);
                }
            }
            assert!(!bounds.is_empty(), "Save profile must be rendered");
            assert!(
                bounds.iter().all(|r| screen.contains_rect(*r)),
                "Save profile outside viewport: {bounds:?}"
            );
        }
    }
}
