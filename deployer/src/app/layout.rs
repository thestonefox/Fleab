use super::*;

pub(super) fn scroll(
    ui: &mut egui::Ui,
    id: &'static str,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::scroll_area::ScrollAreaOutput<()> {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .min_scrolled_height(0.0)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .scroll_source(egui::scroll_area::ScrollSource::ALL)
        .show(ui, |ui| {
            add(ui);
            if !egui::Popup::is_any_open(ui.ctx()) {
                controller_scroll(ui);
            }
            focus(ui);
        })
}

fn controller_scroll(ui: &mut egui::Ui) {
    let delta = ui
        .ctx()
        .data_mut(|data| data.remove_temp::<f32>(egui::Id::new("controller-scroll")))
        .unwrap_or(0.0);
    if delta != 0.0 {
        ui.scroll_with_delta(egui::vec2(0.0, delta));
    }
}

pub(super) fn list_navigation(ui: &mut egui::Ui) {
    controller_scroll(ui);
    focus(ui);
}

fn focus(ui: &mut egui::Ui) {
    if let Some(focused) = ui.memory(|memory| memory.focused())
        && let Some(response) = ui.ctx().read_response(focused)
        && response.layer_id == ui.layer_id()
        && response.gained_focus()
    {
        response.scroll_to_me(Some(egui::Align::Center));
    }
}

pub(super) fn fit_window(ctx: &egui::Context) {
    let Some(monitor) = ctx.input(|input| input.viewport().monitor_size) else {
        return;
    };
    if ctx.data(|data| data.get_temp::<egui::Vec2>(egui::Id::new("fitted-monitor")))
        == Some(monitor)
    {
        return;
    }
    let inner = ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()));
    if let Some(inner) = inner {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("fitted-monitor"), monitor));
        let maximum = (monitor - egui::vec2(32.0, 96.0)).max(egui::vec2(360.0, 240.0));
        if inner.x > maximum.x || inner.y > maximum.y {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(inner.min(maximum)));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(16.0, 16.0)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_and_gamepad_focus_scroll_lower_controls_into_view() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        ctx.style_mut(|style| style.scroll_animation = egui::style::ScrollAnimation::none());
        let mut last = egui::Id::NULL;
        let mut last_rect = egui::Rect::NOTHING;
        let mut viewport = egui::Rect::NOTHING;
        let mut offset = 0.0;
        let mut render = |advance: bool| {
            let events = if advance {
                [true, false]
                    .into_iter()
                    .map(|pressed| egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .collect()
            } else {
                vec![]
            };
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 240.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let result = scroll(ui, "focus-scroll", |ui| {
                            for index in 0..30 {
                                let response = ui.button(format!("Action {index}"));
                                if index == 29 {
                                    last = response.id;
                                    last_rect = response.rect;
                                }
                            }
                        });
                        viewport = result.inner_rect;
                        offset = result.state.offset.y;
                    });
                },
            );
            last
        };
        for _ in 0..3 {
            render(false);
        }
        let target = render(false);
        for _ in 0..40 {
            render(true);
            render(false);
            if ctx.memory(|memory| memory.focused()) == Some(target) {
                break;
            }
        }
        for _ in 0..3 {
            render(false);
        }
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(last));
        assert!(offset > 0.0);
        assert!(
            viewport.contains(last_rect.center()),
            "Focused control remains outside scrolled viewport"
        );
    }

    #[test]
    fn initial_window_fits_720p_at_common_display_scales() {
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            let ctx = egui::Context::default();
            let mut input = egui::RawInput::default();
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.monitor_size = Some(egui::vec2(1280.0, 720.0) / scale);
            viewport.inner_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(780.0, 660.0),
            ));
            let output = ctx.run(input, fit_window);
            let resized = output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .find_map(|command| {
                    if let egui::ViewportCommand::InnerSize(size) = command {
                        Some(*size)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!((resized.y + 96.0) * scale <= 720.1);
            assert!((resized.x + 32.0) * scale <= 1280.1);
        }
    }

    #[test]
    fn short_panels_scroll_with_mouse_drag_and_controller_delta() {
        for input_kind in ["wheel", "drag", "controller"] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            ctx.style_mut(|style| style.animation_time = 0.0);
            let mut offset = 0.0;
            let mut render = |events: Vec<egui::Event>, controller_delta: f32| {
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::new("controller-scroll"), controller_delta)
                });
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(480.0, 240.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let output = scroll(ui, "test-scroll", |ui| {
                                for i in 0..60 {
                                    ui.label(format!("Row {i}"));
                                }
                            });
                            offset = output.state.offset.y;
                            assert!(output.inner_rect.bottom() <= 240.0);
                        });
                    },
                );
                offset
            };
            for _ in 0..3 {
                render(vec![], 0.0);
            }
            match input_kind {
                "wheel" => {
                    render(
                        vec![
                            egui::Event::PointerMoved(egui::pos2(150.0, 150.0)),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -200.0),
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        0.0,
                    );
                }
                "drag" => {
                    render(
                        vec![
                            egui::Event::PointerMoved(egui::pos2(150.0, 180.0)),
                            egui::Event::PointerButton {
                                pos: egui::pos2(150.0, 180.0),
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        0.0,
                    );
                    render(
                        vec![egui::Event::PointerMoved(egui::pos2(150.0, 60.0))],
                        0.0,
                    );
                    render(
                        vec![egui::Event::PointerButton {
                            pos: egui::pos2(150.0, 60.0),
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        0.0,
                    );
                }
                _ => {
                    render(vec![], -200.0);
                }
            }
            let mut final_offset = 0.0;
            for _ in 0..10 {
                final_offset = render(vec![], 0.0);
            }
            assert!(
                final_offset > 0.0,
                "{input_kind} must scroll lower content into view"
            );
        }
    }
}
