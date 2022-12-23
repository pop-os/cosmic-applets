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
    let helper = CosmicAppletHelper::default();
    Window::run(helper.window_settings())
}
