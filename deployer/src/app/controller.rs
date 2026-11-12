use super::*;
use std::time::{Duration, Instant};

const UP: u16 = 1;
const DOWN: u16 = 2;
const LEFT: u16 = 4;
const RIGHT: u16 = 8;
const CONFIRM: u16 = 16;
const BACK: u16 = 32;
const REFRESH: u16 = 64;
const SETTINGS: u16 = 128;
const PREVIOUS: u16 = 256;
const NEXT: u16 = 512;

#[derive(Clone, Copy, Default, PartialEq)]
struct Snapshot {
    connected: bool,
    buttons: u16,
    x: f32,
    y: f32,
    scroll: f32,
}
impl Snapshot {
    fn mask(self) -> u16 {
        self.buttons
            | if self.x < -0.55 {
                LEFT
            } else if self.x > 0.55 {
                RIGHT
            } else {
                0
            }
            | if self.y < -0.55 {
                DOWN
            } else if self.y > 0.55 {
                UP
            } else {
                0
            }
    }
    fn neutral(self) -> bool {
        self.mask() == 0 && self.scroll.abs() < 0.2
    }
}

fn direction(value: f32) -> f32 {
    if value > 0.55 {
        1.0
    } else if value < -0.55 {
        -1.0
    } else {
        0.0
    }
}

struct Hardware {
    snapshot: Arc<Mutex<Result<Snapshot, String>>>,
    focused: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
}
impl Drop for Hardware {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shortcut {
    Back,
    Refresh,
    Settings,
}

pub(super) fn anchor(ctx: &egui::Context, response: &egui::Response) {
    if response.enabled()
        && ctx
            .data(|data| data.get_temp::<bool>(egui::Id::new("controller-focus-anchor")))
            .unwrap_or(false)
    {
        response.request_focus();
        ctx.data_mut(|data| data.remove::<bool>(egui::Id::new("controller-focus-anchor")));
    }
}

#[derive(Default)]
struct Navigation {
    previous: u16,
    active: bool,
    armed: bool,
    repeat: f64,
    last: f64,
}
impl Navigation {
    fn step(&mut self, snapshot: Snapshot, now: f64, active: bool) -> (u16, f32) {
        let delta = (now - self.last).clamp(0.0, 0.05) as f32;
        self.last = now;
        if !active || !snapshot.connected {
            self.previous = 0;
            self.active = false;
            self.armed = false;
            return (0, 0.0);
        }
        if !self.active {
            self.active = true;
            self.armed = snapshot.neutral();
        }
        if !self.armed {
            self.armed = snapshot.neutral();
            return (0, 0.0);
        }
        let held = snapshot.mask();
        let mut pressed = held & !self.previous;
        let directions = UP | DOWN | LEFT | RIGHT | NEXT | PREVIOUS;
        if pressed & directions != 0 {
            self.repeat = now + 0.35;
        } else if held & directions != 0 && now >= self.repeat {
            pressed |= held & directions;
            self.repeat = now + 0.12;
        }
        self.previous = held;
        let scroll = if snapshot.scroll.abs() >= 0.2 {
            snapshot.scroll * 500.0 * delta
        } else {
            0.0
        };
        (pressed, scroll)
    }
}

pub(super) struct State {
    hardware: Option<Hardware>,
    navigation: Navigation,
    clock: Instant,
    pending: Vec<Shortcut>,
    pub connected: bool,
    pub error: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            hardware: None,
            navigation: Navigation::default(),
            clock: Instant::now(),
            pending: Vec::new(),
            connected: false,
            error: None,
        }
    }
}
impl State {
    pub fn start(ctx: &egui::Context) -> Self {
        let snapshot = Arc::new(Mutex::new(Ok(Snapshot::default())));
        let focused = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let (output, active, running, ctx) = (
            snapshot.clone(),
            focused.clone(),
            alive.clone(),
            ctx.clone(),
        );
        thread::spawn(move || {
            let mut gilrs = match gilrs::Gilrs::new() {
                Ok(gilrs) => gilrs,
                Err(error) => {
                    *output.lock().unwrap() = Err(format!("Gamepad input unavailable: {error}"));
                    tray::wake(&ctx);
                    return;
                }
            };
            let mut previous = Snapshot::default();
            while running.load(Ordering::Relaxed) {
                while gilrs.next_event().is_some() {}
                let mut current = Snapshot::default();
                for (_, gamepad) in gilrs.gamepads() {
                    use gilrs::{Axis, Button};
                    let mut candidate = Snapshot {
                        connected: true,
                        x: direction(gamepad.value(Axis::LeftStickX)),
                        y: direction(gamepad.value(Axis::LeftStickY)),
                        scroll: if gamepad.value(Axis::RightStickY).abs() >= 0.2 {
                            gamepad.value(Axis::RightStickY)
                        } else {
                            0.0
                        },
                        ..Default::default()
                    };
                    for (button, mask) in [
                        (Button::DPadUp, UP),
                        (Button::DPadDown, DOWN),
                        (Button::DPadLeft, LEFT),
                        (Button::DPadRight, RIGHT),
                        (Button::South, CONFIRM),
                        (Button::East, BACK),
                        (Button::West, REFRESH),
                        (Button::North, SETTINGS),
                        (Button::LeftTrigger, PREVIOUS),
                        (Button::RightTrigger, NEXT),
                        (Button::Start, SETTINGS),
                    ] {
                        if gamepad.is_pressed(button) {
                            candidate.buttons |= mask;
                        }
                    }
                    if !current.connected || !candidate.neutral() {
                        current = candidate;
                    }
                    if !candidate.neutral() {
                        break;
                    }
                }
                *output.lock().unwrap() = Ok(current);
                if active.load(Ordering::Relaxed)
                    && (current != previous
                        || current.mask() & (UP | DOWN | LEFT | RIGHT | NEXT | PREVIOUS) != 0
                        || current.scroll.abs() >= 0.2)
                {
                    tray::wake(&ctx);
                }
                previous = current;
                gilrs.inc();
                thread::sleep(if current.connected {
                    Duration::from_millis(16)
                } else {
                    Duration::from_millis(200)
                });
            }
        });
        Self {
            hardware: Some(Hardware {
                snapshot,
                focused,
                alive,
            }),
            ..Default::default()
        }
    }

    pub fn input(&mut self, ctx: &egui::Context, input: &mut egui::RawInput, active: bool) {
        let Some(hardware) = &self.hardware else {
            return;
        };
        hardware.focused.store(active, Ordering::Relaxed);
        let snapshot = match &*hardware.snapshot.lock().unwrap() {
            Ok(snapshot) => *snapshot,
            Err(error) => {
                self.error = Some(error.clone());
                return;
            }
        };
        self.connected = snapshot.connected;
        let (pressed, scroll) =
            self.navigation
                .step(snapshot, self.clock.elapsed().as_secs_f64(), active);
        self.translate(ctx, input, pressed, scroll);
    }

    fn translate(
        &mut self,
        ctx: &egui::Context,
        input: &mut egui::RawInput,
        pressed: u16,
        scroll: f32,
    ) {
        let popup = egui::Popup::is_any_open(ctx);
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("controller-scroll"), scroll));
        let mut focused = ctx.memory(|memory| memory.focused().is_some());
        if !popup && ((!focused && pressed != 0) || pressed & SETTINGS != 0) {
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("controller-focus-anchor"), true));
        }
        for (mask, key) in [
            (UP, egui::Key::ArrowUp),
            (DOWN, egui::Key::ArrowDown),
            (LEFT, egui::Key::ArrowLeft),
            (RIGHT, egui::Key::ArrowRight),
            (CONFIRM, egui::Key::Enter),
            (BACK, egui::Key::Escape),
            (PREVIOUS, egui::Key::Tab),
            (NEXT, egui::Key::Tab),
        ] {
            if pressed & mask == 0 {
                continue;
            }
            let key = if !focused && mask != BACK {
                focused = true;
                egui::Key::Tab
            } else {
                key
            };
            let modifiers = egui::Modifiers {
                shift: mask == PREVIOUS,
                ..Default::default()
            };
            for down in [true, false] {
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: down,
                    repeat: false,
                    modifiers,
                });
            }
        }
        if !popup {
            for (mask, shortcut) in [
                (BACK, Shortcut::Back),
                (REFRESH, Shortcut::Refresh),
                (SETTINGS, Shortcut::Settings),
            ] {
                if pressed & mask != 0 {
                    self.pending.push(shortcut);
                }
            }
        }
        if pressed != 0 || scroll != 0.0 {
            ctx.request_repaint();
        }
    }

    pub fn shortcuts(&mut self) -> Vec<Shortcut> {
        std::mem::take(&mut self.pending)
    }
    pub fn hint(&self, ui: &mut egui::Ui) {
        if self.connected {
            design::caption(
                ui,
                "D-pad / left stick: move · A: select · B: back · LB/RB: focus · right stick: scroll · X: refresh · Y: settings",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_and_dropdown_selection_work_without_pointer_events() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut controller = State::default();
        let mut clicks = 0;
        let mut selected = 0;
        let mut ids = (egui::Id::NULL, egui::Id::NULL);
        let mut render = |mask: u16| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 360.0),
                )),
                ..Default::default()
            };
            controller.translate(&ctx, &mut input, mask, 0.0);
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let button = ui.button("Select");
                    ids.0 = button.id;
                    if button.clicked() {
                        clicks += 1;
                    }
                    let combo = egui::ComboBox::from_id_salt("gamepad-choice")
                        .selected_text(format!("Choice {selected}"))
                        .show_ui(ui, |ui| {
                            for index in 0..3 {
                                if ui
                                    .selectable_value(
                                        &mut selected,
                                        index,
                                        format!("Choice {index}"),
                                    )
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                            layout::list_navigation(ui);
                        });
                    ids.1 = combo.response.id;
                });
            });
            (clicks, selected, ids)
        };
        for _ in 0..3 {
            render(0);
        }
        let (_, _, ids) = render(0);
        ctx.memory_mut(|memory| memory.request_focus(ids.0));
        assert_eq!(render(CONFIRM).0, 1);
        ctx.memory_mut(|memory| memory.request_focus(ids.1));
        render(CONFIRM);
        assert!(egui::Popup::is_any_open(&ctx));
        render(0);
        render(DOWN);
        render(0);
        render(DOWN);
        render(0);
        assert_eq!(
            render(CONFIRM).1,
            1,
            "Gamepad must be able to choose another dropdown entry"
        );
        assert!(!egui::Popup::is_any_open(&ctx));
    }
    #[test]
    fn input_repeats_navigation_but_never_repeats_confirmation() {
        let mut nav = Navigation::default();
        let mut state = Snapshot {
            connected: true,
            ..Default::default()
        };
        nav.step(state, 0.0, true);
        state.buttons = DOWN | CONFIRM;
        assert_eq!(nav.step(state, 0.1, true).0, DOWN | CONFIRM);
        assert_eq!(nav.step(state, 0.2, true).0, 0);
        assert_eq!(nav.step(state, 0.5, true).0, DOWN);
        assert_eq!(nav.step(state, 0.6, false).0, 0);
        assert_eq!(
            nav.step(state, 0.7, true).0,
            0,
            "Held button must not activate after regaining focus"
        );
        state.buttons = 0;
        nav.step(state, 0.8, true);
        state.buttons = CONFIRM;
        assert_eq!(nav.step(state, 0.9, true).0, CONFIRM);
    }
    #[test]
    fn stick_dead_zone_disconnect_and_scroll_are_handled() {
        let mut nav = Navigation::default();
        let mut state = Snapshot {
            connected: true,
            x: 0.1,
            ..Default::default()
        };
        assert_eq!(nav.step(state, 0.0, true), (0, 0.0));
        state.x = 0.8;
        state.scroll = -1.0;
        let (buttons, scroll) = nav.step(state, 0.05, true);
        assert_eq!(buttons, RIGHT);
        assert!(scroll < 0.0);
        state.connected = false;
        assert_eq!(nav.step(state, 0.1, true), (0, 0.0));
    }
}
