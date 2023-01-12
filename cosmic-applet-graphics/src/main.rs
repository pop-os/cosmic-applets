mod dbus;
mod graphics;
mod localize;
mod window;

use cosmic::{
    applet::{cosmic_panel_config::PanelAnchor, CosmicAppletHelper},
    iced::{wayland::InitialSurface, Application, Settings},
    iced_native::layout::Limits,
};

use window::*;

pub fn main() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    let mut settings: Settings<()> = helper.window_settings();
    match helper.anchor {
        PanelAnchor::Top | PanelAnchor::Bottom => {
            match &mut settings.initial_surface {
                InitialSurface::LayerSurface(_) => todo!(),
                InitialSurface::XdgWindow(w) => {
                    w.autosize = true;
                    w.size_limits = Limits::NONE
                        .min_height(1)
                        .max_height(200)
                        .min_width(1)
                        .max_width(1000);
                }
            };
        }
        _ => {}
    };
    Window::run(settings)
}
