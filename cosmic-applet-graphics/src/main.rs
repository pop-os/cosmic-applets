mod dbus;
mod graphics;
mod localize;
mod window;

use cosmic::{
    applet::{cosmic_panel_config::PanelAnchor, CosmicAppletHelper},
    iced::{wayland::InitialSurface, Application, Settings},
    iced_runtime::core::layout::Limits,
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
                        .min_height(1.0)
                        .max_height(200.0)
                        .min_width(1.0)
                        .max_width(1000.0);
                }
                InitialSurface::None => unimplemented!(),
            };
        }
        _ => {}
    };
    Window::run(settings)
}
