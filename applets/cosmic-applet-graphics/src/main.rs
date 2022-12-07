mod dbus;
mod graphics;
mod window;

use cosmic::{
    iced::{sctk_settings::InitialSurface, Application},
    iced_native::command::platform_specific::wayland::window::SctkWindowSettings,
    iced_native::window::Settings,
    settings, applet::CosmicAppletHelper,
};
use cosmic_panel_config::PanelSize;
use window::*;

pub fn main() -> cosmic::iced::Result {
    let mut settings = settings();
    let helper = CosmicAppletHelper::default();
    let pixels = helper.suggested_icon_size() as u32;
    settings.initial_surface = InitialSurface::XdgWindow(SctkWindowSettings {
        iced_settings: Settings {
            size: (pixels + 16, pixels + 16),
            min_size: Some((pixels + 16, pixels + 16)),
            max_size: Some((pixels + 16, pixels + 16)),
            ..Default::default()
        },
        ..Default::default()
    });
    Window::run(settings)
}
