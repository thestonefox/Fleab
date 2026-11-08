#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    eframe::run_native(
        "Fleab",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(Foundation))),
    )
}

struct Foundation;
impl eframe::App for Foundation {
    fn update(&mut self, ctx: &eframe::egui::Context, _: &mut eframe::Frame) {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Fleab");
            ui.label("Build it. Hop it. Play it.");
        });
    }
}
