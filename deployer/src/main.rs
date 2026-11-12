#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    match fleab::updates::install::bootstrap() {
        Ok(true) => return Ok(()),
        Ok(false) => (),
        Err(error) => return Err(eframe::Error::AppCreation(error.into())),
    }
    let primary = match fleab::instance::claim() {
        Ok(fleab::instance::Claim::Primary(primary)) => primary,
        Ok(fleab::instance::Claim::Activated) => return Ok(()),
        Err(error) => return Err(eframe::Error::AppCreation(error.into())),
    };
    let title = format!("Fleab {}", env!("CARGO_PKG_VERSION"));
    let icon = image::load_from_memory(include_bytes!("../../assets/fleab.png"))
        .expect("Bundled application icon")
        .into_rgba8();
    let (width, height) = icon.dimensions();
    let options = eframe::NativeOptions {
        #[cfg(target_os = "linux")]
        event_loop_builder: Some(Box::new(|builder| {
            // winit cannot hide/unhide a native Wayland window. Prefer X11
            // (including XWayland) when available so tray restoration works.
            if std::env::var_os("DISPLAY").is_some() {
                use winit::platform::x11::EventLoopBuilderExtX11;
                builder.with_x11();
            }
        })),
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(&title)
            .with_decorations(true)
            .with_inner_size([780.0, 660.0])
            .with_min_inner_size([360.0, 240.0])
            .with_clamp_size_to_monitor_size(true)
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
        Box::new(move |cc| {
            let mut app = fleab::app::FleabApp::with_instance(&cc.egui_ctx, primary);
            app.attach_window(cc);
            Ok(Box::new(app))
        }),
    )
}
