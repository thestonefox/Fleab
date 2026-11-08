use ashpd::desktop::settings::{ColorScheme, Settings};
use eframe::egui::{Context, Id, RawInput, Theme};
use futures_util::StreamExt;

const KEY: &str = "linux-system-appearance";

// X11 does not provide theme events through winit. Use the standard read-only
// desktop portal on both X11 and Wayland, without blocking the UI thread.
pub(super) fn watch(ctx: &Context) {
    let ctx = ctx.clone();
    let _ = std::thread::Builder::new()
        .name("system-appearance".into())
        .spawn(move || {
            loop {
                let _ = pollster::block_on(receive(&ctx));
                // A missing/restarted portal must not prevent launching the app.
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
        });
}

async fn receive(ctx: &Context) -> ashpd::Result<()> {
    let settings = Settings::new().await?;
    // Subscribe first so a theme change during the initial read is not lost.
    let changes = settings.receive_color_scheme_changed().await?;
    futures_util::pin_mut!(changes);
    publish(ctx, settings.color_scheme().await?);
    while let Some(scheme) = changes.next().await {
        publish(ctx, scheme);
    }
    Ok(())
}

fn publish(ctx: &Context, scheme: ColorScheme) {
    let theme = match scheme {
        ColorScheme::PreferDark => Some(Theme::Dark),
        ColorScheme::PreferLight => Some(Theme::Light),
        ColorScheme::NoPreference => None,
    };
    ctx.data_mut(|data| data.insert_temp(Id::new(KEY), theme));
    ctx.request_repaint();
}

pub(super) fn system_input(ctx: &Context, input: &mut RawInput) {
    if let Some(Some(theme)) = ctx.data(|data| data.get_temp::<Option<Theme>>(Id::new(KEY))) {
        input.system_theme = Some(theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portal_updates_system_input_and_releases_native_preference() {
        let ctx = Context::default();
        super::super::design::install(&ctx);
        for (scheme, native, expected) in [
            (ColorScheme::PreferDark, Theme::Light, Theme::Dark),
            (ColorScheme::PreferLight, Theme::Dark, Theme::Light),
            (ColorScheme::NoPreference, Theme::Dark, Theme::Dark),
        ] {
            publish(&ctx, scheme);
            let mut input = RawInput {
                system_theme: Some(native),
                ..Default::default()
            };
            system_input(&ctx, &mut input);
            let _ = ctx.run(input, |ctx| {
                assert_eq!(ctx.theme(), expected);
                assert_eq!(
                    ctx.options(|options| options.theme_preference),
                    eframe::egui::ThemePreference::System
                );
            });
        }
    }
}
