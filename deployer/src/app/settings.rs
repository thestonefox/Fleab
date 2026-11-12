use super::icons::Icon;
use super::*;
use crate::profiles::RunnerKind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Folders,
    Launch,
    Updates,
}

pub(super) struct Editor {
    profile: Profile,
    existing: bool,
    game_args: String,
    runner_args: String,
    error: String,
    tab: Tab,
}

impl Editor {
    fn unsaved(&self, config: &Config) -> bool {
        let mut profile = self.profile.clone();
        profile.launch.game_arguments = self.game_args.lines().map(str::to_owned).collect();
        profile.launch.runner_arguments = self.runner_args.lines().map(str::to_owned).collect();
        profile.destinations.retain(|_, value| !value.is_empty());
        profile
            .executable_overrides
            .retain(|_, value| !value.is_empty());
        if let Some(saved) = config.profiles.iter().find(|p| p.id == profile.id) {
            let mut saved = saved.clone();
            saved.destinations.retain(|_, value| !value.is_empty());
            saved
                .executable_overrides
                .retain(|_, value| !value.is_empty());
            return profile != saved;
        }
        let blank = Profile {
            id: profile.id,
            ..Default::default()
        };
        profile != blank
    }
    pub fn new(profile: Profile, existing: bool) -> Self {
        Self {
            game_args: profile.launch.game_arguments.join("\n"),
            runner_args: profile.launch.runner_arguments.join("\n"),
            profile,
            existing,
            error: String::new(),
            tab: Tab::Folders,
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(label);
    ui.add(
        egui::TextEdit::singleline(value)
            .margin(egui::Margin::symmetric(10, 8))
            .desired_width(f32::INFINITY)
            .hint_text(hint),
    );
}

fn folder(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.horizontal(|ui| {
        // TextEdit's desired width excludes its 20px horizontal padding.
        let width = (ui.available_width() - 64.0).max(60.0);
        ui.add(
            egui::TextEdit::singleline(value)
                .margin(egui::Margin::symmetric(10, 8))
                .desired_width(width)
                .hint_text("Not configured"),
        );
        if icons::button(ui, Icon::Folder, &format!("Choose {label}"), true).clicked()
            && let Some(path) = rfd::FileDialog::new().set_title(label).pick_folder()
        {
            *value = path.to_string_lossy().into();
        }
    });
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(16.0).strong());
    ui.add_space(4.0);
}

impl FleabApp {
    pub(super) fn settings(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        let mut open = true;
        let mut action = None;
        egui::TopBottomPanel::top("settings-header")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if icons::button(ui, Icon::Back, "Back to builds", !self.updates.busy())
                        .clicked()
                    {
                        action = Some("cancel");
                    }
                    ui.add_space(4.0);
                    ui.heading("Settings");
                });
                ui.add_space(4.0);
                design::caption(
                    ui,
                    "Manage profiles, launch options and application updates.",
                );
            });
        egui::TopBottomPanel::bottom("settings-actions")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ctx, |ui| {
                if !editor.error.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(64.0)
                        .show(ui, |ui| {
                            ui.colored_label(ui.visuals().error_fg_color, &editor.error);
                        });
                    ui.add_space(8.0);
                }
                ui.horizontal(|ui| {
                    if editor.tab == Tab::Updates {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(!self.updates.busy(), egui::Button::new("Done"))
                                .clicked()
                            {
                                action = Some("cancel");
                            }
                        });
                        return;
                    }
                    if editor.existing
                        && ui
                            .button("Delete profile")
                            .on_hover_text("Remove this profile; installed files are kept")
                            .clicked()
                    {
                        action = Some("delete");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if design::primary(ui, "Save profile", true).clicked() {
                            action = Some("save");
                        }
                        if ui.button("Cancel").clicked() {
                            action = Some("cancel");
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill).inner_margin(egui::Margin::symmetric(24, 8)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().id_salt("settings-scroll").show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(860.0));
                    if editor.tab != Tab::Updates { design::card(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label("Game profile");
                        egui::ComboBox::from_id_salt("edit-profile").width(ui.available_width()).truncate()
                            .selected_text(&editor.profile.name).show_ui(ui, |ui| {
                                for profile in &self.config.profiles {
                                    if ui.selectable_label(profile.id == editor.profile.id, &profile.name).clicked() {
                                        editor = Editor::new(profile.clone(), true);
                                    }
                                }
                            });
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("New profile").clicked() { action = Some("new"); }
                            if ui.button("Import…").on_hover_text("Import a profile from JSON").clicked() { action = Some("import"); }
                            if ui.button("Export…")
                                .on_hover_text("Export this profile as JSON").clicked() { action = Some("export"); }
                        });
                    });
                    ui.add_space(8.0); }
                    ui.add_enabled_ui(!self.updates.busy(), |ui| { ui.horizontal(|ui| {
                        ui.selectable_value(&mut editor.tab, Tab::Folders, "Profile & folders");
                        ui.selectable_value(&mut editor.tab, Tab::Launch, "Launch options");
                        ui.selectable_value(&mut editor.tab, Tab::Updates, "Updates");
                    }); });
                    ui.add_space(8.0);
                    match editor.tab {
                        Tab::Updates => {
                            let unsaved = editor.unsaved(&self.config);
                            if unsaved { design::caption(ui, "Save or cancel your profile changes before installing an update."); }
                            self.updates.settings(ctx, ui, !self.read_only && self.worker.is_none() && !unsaved, &self.config);
                        }
                        Tab::Folders => {
                            design::card(ui).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                section(ui, "Profile");
                                field(ui, "Name", &mut editor.profile.name, "Name your game");
                                ui.add_space(8.0);
                                folder(ui, "Published archives", &mut editor.profile.source);
                                design::caption(ui, "The local folder or mounted share where Unity publishes builds.");
                            });
                            ui.add_space(8.0);
                            design::card(ui).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                section(ui, "Client destinations");
                                design::caption(ui, "Only configured targets appear in your build list.");
                                for target in [Target::WindowsClient, Target::LinuxClient, Target::MacClient] {
                                    folder(ui, target.label(), editor.profile.destinations.entry(target).or_default());
                                }
                                ui.add_space(8.0);
                                egui::CollapsingHeader::new("Server destinations").show(ui, |ui| {
                                    for target in [Target::WindowsServer, Target::LinuxServer, Target::MacServer] {
                                        folder(ui, target.label(), editor.profile.destinations.entry(target).or_default());
                                    }
                                });
                            });
                        }
                        Tab::Launch => {
                            design::card(ui).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                section(ui, "Launch arguments");
                                design::caption(ui, "One argument per line. Spaces stay inside an argument; no shell quotes needed.");
                                ui.add(egui::TextEdit::multiline(&mut editor.game_args).desired_rows(3)
                                    .desired_width(f32::INFINITY).hint_text("Optional game or server arguments"));
                                ui.add_space(8.0);
                                egui::CollapsingHeader::new("Executable overrides").show(ui, |ui| {
                                    design::caption(ui, "Leave blank for automatic detection. Paths are relative to the deployment folder.");
                                    for target in Target::ALL {
                                        field(ui, target.label(), editor.profile.executable_overrides.entry(target).or_default(), "Automatic detection");
                                    }
                                });
                            });
                            ui.add_space(8.0);
                            design::card(ui).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                section(ui, "Windows builds on Linux");
                                design::caption(ui, "Use Wine or Proton to launch Windows builds on a Linux computer.");
                                egui::CollapsingHeader::new("Compatibility settings").show(ui, |ui| {
                                    let label = if editor.profile.launch.runner.is_empty() { "Automatic discovery".into() }
                                        else { editor.profile.launch.runner.clone() };
                                    egui::ComboBox::from_id_salt("runner").width(ui.available_width()).truncate()
                                        .selected_text(label).show_ui(ui, |ui| {
                                            if ui.selectable_label(editor.profile.launch.runner.is_empty(), "Automatic discovery").clicked() {
                                                editor.profile.launch.runner.clear(); editor.profile.launch.runner_kind = RunnerKind::Auto;
                                            }
                                            for runner in &self.runners {
                                                if ui.selectable_label(editor.profile.launch.runner == runner.path.to_string_lossy(), &runner.name).clicked() {
                                                    editor.profile.launch.runner = runner.path.to_string_lossy().into();
                                                    editor.profile.launch.runner_kind = runner.kind;
                                                    editor.profile.launch.steam_root = runner.steam_root.to_string_lossy().into();
                                                }
                                            }
                                        });
                                    ui.label("Runner type");
                                    egui::ComboBox::from_id_salt("runner-kind").selected_text(format!("{:?}", editor.profile.launch.runner_kind))
                                        .show_ui(ui, |ui| { for kind in [RunnerKind::Auto, RunnerKind::Wine, RunnerKind::Proton] {
                                            ui.selectable_value(&mut editor.profile.launch.runner_kind, kind, format!("{kind:?}"));
                                        }});
                                    field(ui, "Runner executable", &mut editor.profile.launch.runner, "Optional Wine or Proton path");
                                    if ui.button("Choose executable…").clicked()
                                        && let Some(path) = rfd::FileDialog::new().pick_file() {
                                        editor.profile.launch.runner = path.to_string_lossy().into();
                                    }
                                    folder(ui, "Compatibility prefix / Proton data", &mut editor.profile.launch.prefix);
                                    folder(ui, "Steam root (for manual Proton)", &mut editor.profile.launch.steam_root);
                                    ui.label("Runner arguments · one per line");
                                    ui.add(egui::TextEdit::multiline(&mut editor.runner_args).desired_rows(2)
                                        .desired_width(f32::INFINITY).hint_text("Proton defaults to run"));
                                });
                            });
                        }
                    }
                    ui.add_space(8.0);
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
    fn text_bounds(shape: &egui::epaint::Shape, label: &str, found: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                found.push(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    text_bounds(shape, label, found);
                }
            }
            _ => (),
        }
    }
    #[test]
    fn settings_save_stays_inside_small_windows() {
        for (size, theme) in [egui::vec2(700.0, 490.0), egui::vec2(480.0, 390.0)]
            .into_iter()
            .flat_map(|size| [egui::Theme::Light, egui::Theme::Dark].map(|theme| (size, theme)))
        {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let mut app = FleabApp {
                config: Config::default(),
                config_path: None,
                _config_lock: None,
                read_only: false,
                builds: Vec::new(),
                selected_build: 0,
                action: Action::default(),
                targets: BTreeMap::new(),
                runners: Vec::new(),
                status: String::new(),
                error: false,
                worker: None,
                editor: Some(Editor::new(Profile::default(), true)),
                children: Vec::new(),
                updates: update_view::Updates::default(),
            };
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            // egui windows settle their auto-size on a second layout pass.
            let mut bounds = Vec::new();
            let mut controls = Vec::new();
            for _ in 0..3 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        system_theme: Some(theme),
                        ..Default::default()
                    },
                    |ctx| app.settings(ctx),
                );
                bounds.clear();
                controls.clear();
                for shape in &output.shapes {
                    text_bounds(&shape.shape, "Save profile", &mut bounds);
                    if let egui::Shape::Rect(rect) = &shape.shape {
                        controls.push(rect.rect);
                    }
                }
            }
            assert!(!bounds.is_empty(), "Save profile must be rendered");
            assert!(
                bounds.iter().all(|r| screen.contains_rect(*r)),
                "Save profile outside viewport: {bounds:?}"
            );
            assert!(
                controls
                    .iter()
                    .all(|r| r.left() >= -0.5 && r.right() <= screen.right() + 0.5),
                "Settings controls extend past {screen:?}: {controls:?}"
            );
        }
    }

    #[test]
    fn empty_destination_fields_do_not_block_application_updates() {
        let profile = Profile::default();
        let config = Config {
            schema_version: 1,
            profiles: vec![profile.clone()],
            selected_profile: Some(profile.id),
        };
        let mut editor = Editor::new(profile, true);
        for target in Target::ALL {
            editor.profile.destinations.entry(target).or_default();
            editor
                .profile
                .executable_overrides
                .entry(target)
                .or_default();
        }
        assert!(!editor.unsaved(&config));
        editor.profile.source = "a changed source".into();
        assert!(editor.unsaved(&config));
        assert!(!Editor::new(Profile::default(), false).unsaved(&Config::default()));
    }

    #[test]
    fn updates_page_has_a_visible_done_action_without_a_profile() {
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let mut editor = Editor::new(Profile::default(), false);
            editor.tab = Tab::Updates;
            let mut app = FleabApp {
                config: Config::default(),
                config_path: None,
                _config_lock: None,
                read_only: false,
                builds: Vec::new(),
                selected_build: 0,
                action: Action::default(),
                targets: BTreeMap::new(),
                runners: Vec::new(),
                status: String::new(),
                error: false,
                worker: None,
                editor: Some(editor),
                children: Vec::new(),
                updates: update_view::Updates::default(),
            };
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 390.0));
            let mut done = Vec::new();
            for _ in 0..3 {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        system_theme: Some(theme),
                        ..Default::default()
                    },
                    |ctx| app.settings(ctx),
                );
                done.clear();
                for shape in output.shapes {
                    text_bounds(&shape.shape, "Done", &mut done);
                    if let egui::Shape::Rect(rect) = shape.shape {
                        assert!(rect.rect.right() <= screen.right() + 0.5);
                    }
                }
            }
            assert!(!done.is_empty() && done.iter().all(|r| screen.contains_rect(*r)));
        }
    }
}
