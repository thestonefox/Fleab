use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Stroke};

pub(super) fn install(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::System);
    ctx.options_mut(|options| options.fallback_theme = egui::Theme::Light);
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.style_mut_of(theme, |style| {
            let dark = theme == egui::Theme::Dark;
            let background = if dark { 0x202020 } else { 0xf3f3f3 };
            let surface = if dark { 0x2b2b2b } else { 0xffffff };
            let control = if dark { 0x333333 } else { 0xfbfbfb };
            let border = if dark { 0x454545 } else { 0xd6d6d6 };
            let text = if dark { 0xf5f5f5 } else { 0x1a1a1a };
            let secondary = if dark { 0xc7c7c7 } else { 0x5d5d5d };
            let accent = if dark { 0x60cdff } else { 0x0067c0 };
            style
                .text_styles
                .insert(egui::TextStyle::Body, FontId::proportional(14.0));
            style
                .text_styles
                .insert(egui::TextStyle::Button, FontId::proportional(14.0));
            style
                .text_styles
                .insert(egui::TextStyle::Small, FontId::proportional(12.0));
            style
                .text_styles
                .insert(egui::TextStyle::Heading, FontId::proportional(26.0));
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(14.0, 8.0);
            style.spacing.interact_size.y = 34.0;
            style.spacing.combo_height = 260.0;
            style.spacing.scroll = egui::style::ScrollStyle::thin();
            let visuals = &mut style.visuals;
            visuals.panel_fill = color(background);
            visuals.window_fill = color(surface);
            visuals.extreme_bg_color = color(surface);
            visuals.text_edit_bg_color = Some(color(surface));
            visuals.faint_bg_color = color(control);
            visuals.weak_text_color = Some(color(secondary));
            visuals.hyperlink_color = color(accent);
            visuals.error_fg_color = color(if dark { 0xff99a4 } else { 0xc42b1c });
            visuals.window_corner_radius = 8.into();
            visuals.menu_corner_radius = 8.into();
            visuals.window_stroke = Stroke::new(1.0_f32, color(border));
            visuals.selection.bg_fill = color(if dark { 0x254c60 } else { 0xcde6f7 });
            visuals.selection.stroke = Stroke::new(1.0_f32, color(text));
            for widget in [
                &mut visuals.widgets.noninteractive,
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
                &mut visuals.widgets.open,
            ] {
                widget.corner_radius = 4.into();
                widget.fg_stroke = Stroke::new(1.0_f32, color(text));
                widget.bg_stroke = Stroke::new(1.0_f32, color(border));
                widget.expansion = 0.0;
            }
            visuals.widgets.inactive.bg_fill = color(control);
            visuals.widgets.inactive.weak_bg_fill = color(control);
            visuals.widgets.hovered.bg_fill = color(if dark { 0x3c3c3c } else { 0xf1f1f1 });
            visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
            visuals.widgets.active.bg_fill = color(if dark { 0x303030 } else { 0xe9e9e9 });
            visuals.widgets.active.weak_bg_fill = visuals.widgets.active.bg_fill;
            visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, color(accent));
        });
    }
    install_system_font(ctx);
}

fn color(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn install_system_font(ctx: &egui::Context) {
    // Read installed fonts, never redistribute platform font files.
    let candidates: Vec<std::path::PathBuf> = if cfg!(windows) {
        let windows = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
        ["SegUIVar.ttf", "segoeui.ttf"]
            .into_iter()
            .map(|name| std::path::PathBuf::from(&windows).join("Fonts").join(name))
            .collect()
    } else if cfg!(target_os = "macos") {
        [
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    } else {
        [
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    };
    if let Some(bytes) = candidates.iter().find_map(|path| std::fs::read(path).ok()) {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("system-ui".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "system-ui".into());
        ctx.set_fonts(fonts);
    }
}

pub(super) fn card(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(Stroke::new(
            1.0_f32,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(8)
        .inner_margin(20)
}

pub(super) fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    primary_button(
        ui,
        egui::Button::new(label).min_size(egui::vec2(112.0, 36.0)),
        enabled,
    )
}

pub(super) fn primary_button(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    enabled: bool,
) -> egui::Response {
    if !enabled {
        return ui.add_enabled(false, button);
    }
    let dark = ui.visuals().dark_mode;
    ui.scope(|ui| {
        let visuals = ui.visuals_mut();
        for (widget, fill) in [
            (
                &mut visuals.widgets.inactive,
                if dark { 0x60cdff } else { 0x0067c0 },
            ),
            (
                &mut visuals.widgets.hovered,
                if dark { 0x79d5ff } else { 0x005fb0 },
            ),
            (
                &mut visuals.widgets.active,
                if dark { 0x4bb8ec } else { 0x00549d },
            ),
        ] {
            widget.bg_fill = color(fill);
            widget.weak_bg_fill = color(fill);
            widget.bg_stroke = Stroke::NONE;
            widget.fg_stroke.color = color(if dark { 0x00334d } else { 0xffffff });
        }
        ui.add_enabled(enabled, button)
    })
    .inner
}

pub(super) fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(ui.visuals().weak_text_color()));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follows_system_theme_changes_without_losing_custom_styles() {
        let ctx = egui::Context::default();
        install(&ctx);
        for theme in [egui::Theme::Light, egui::Theme::Dark, egui::Theme::Light] {
            let _ = ctx.run(
                egui::RawInput {
                    system_theme: Some(theme),
                    ..Default::default()
                },
                |ctx| {
                    assert_eq!(ctx.theme(), theme);
                    assert_eq!(
                        ctx.options(|options| options.theme_preference),
                        egui::ThemePreference::System
                    );
                    assert_eq!(ctx.style().spacing.interact_size.y, 34.0);
                    assert_eq!(ctx.style().visuals.dark_mode, theme == egui::Theme::Dark);
                },
            );
        }
    }
}
