#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let icon = image::load_from_memory(include_bytes!("../../assets/fleab.png"))
        .expect("Bundled application icon")
        .into_rgba8();
    let (width, height) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([700.0, 490.0])
            .with_min_inner_size([480.0, 390.0])
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "Fleab",
        options,
        Box::new(|cc| Ok(Box::new(fleab::app::FleabApp::new(&cc.egui_ctx)))),
    )
}
