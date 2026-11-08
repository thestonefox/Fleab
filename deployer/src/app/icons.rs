use eframe::egui::{self, Color32, Pos2, Response, Stroke, Ui, Vec2};

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Settings,
    Refresh,
    Folder,
    Back,
    Play,
}

pub(super) fn button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool) -> Response {
    let response = ui.add_enabled(enabled, egui::Button::new("").min_size(Vec2::splat(36.0)));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    paint(
        ui,
        icon,
        response.rect.center(),
        18.0,
        ui.style()
            .interact(&response)
            .fg_stroke
            .color
            .gamma_multiply(if enabled {
                1.0
            } else {
                ui.visuals().disabled_alpha()
            }),
    );
    response.on_hover_text(label)
}

pub(super) fn text_button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool) -> Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(format!("     {label}")).min_size(Vec2::new(100.0, 36.0)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    paint(
        ui,
        icon,
        Pos2::new(response.rect.left() + 17.0, response.rect.center().y),
        16.0,
        ui.style()
            .interact(&response)
            .fg_stroke
            .color
            .gamma_multiply(if enabled {
                1.0
            } else {
                ui.visuals().disabled_alpha()
            }),
    );
    response
}

pub(super) fn paint(ui: &Ui, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.5_f32, color);
    let point = |x: f32, y: f32| center + Vec2::new(x, y) * size / 20.0;
    match icon {
        Icon::Settings => {
            let points = (0..32)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / 32.0;
                    let radius = if i % 4 == 1 || i % 4 == 2 { 9.0 } else { 7.0 };
                    point(angle.cos() * radius, angle.sin() * radius)
                })
                .collect();
            painter.add(egui::Shape::closed_line(points, stroke));
            painter.circle_stroke(center, size * 0.16, stroke);
        }
        Icon::Refresh => {
            let points = (0..=24)
                .map(|i| {
                    let angle = -0.6 + i as f32 * 5.0 / 24.0;
                    point(angle.cos() * 7.0, angle.sin() * 7.0)
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            painter.add(egui::Shape::line(
                vec![point(2.0, -5.0), point(6.0, -4.0), point(7.0, -8.0)],
                stroke,
            ));
        }
        Icon::Folder => {
            painter.add(egui::Shape::closed_line(
                vec![
                    point(-8.0, -6.0),
                    point(-2.0, -6.0),
                    point(0.0, -3.0),
                    point(8.0, -3.0),
                    point(8.0, 7.0),
                    point(-8.0, 7.0),
                ],
                stroke,
            ));
        }
        Icon::Back => {
            painter.line_segment([point(8.0, 0.0), point(-7.0, 0.0)], stroke);
            painter.add(egui::Shape::line(
                vec![point(-1.0, -6.0), point(-7.0, 0.0), point(-1.0, 6.0)],
                stroke,
            ));
        }
        Icon::Play => {
            painter.add(egui::Shape::closed_line(
                vec![point(-5.0, -7.0), point(7.0, 0.0), point(-5.0, 7.0)],
                stroke,
            ));
        }
    }
}
