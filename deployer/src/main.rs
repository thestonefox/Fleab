#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    match fleab::updates::install::bootstrap() {
        Ok(true) => return Ok(()),
        Ok(false) => (),
        Err(error) => return Err(eframe::Error::AppCreation(error.into())),
    }
    let title = format!("Fleab {}", env!("CARGO_PKG_VERSION"));
    let icon = image::load_from_memory(include_bytes!("../../assets/fleab.png"))
        .expect("Bundled application icon")
        .into_rgba8();
    let (width, height) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(&title)
            .with_decorations(true)
            .with_inner_size([780.0, 660.0])
            .with_min_inner_size([480.0, 390.0])
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            }),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(|cc| Ok(Box::new(fleab::app::FleabApp::new(&cc.egui_ctx)))),
    )
}
