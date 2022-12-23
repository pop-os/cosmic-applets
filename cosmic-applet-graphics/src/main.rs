mod dbus;
mod graphics;
mod window;

use cosmic::{
    iced::Application, applet::CosmicAppletHelper,
};
use window::*;

pub fn main() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    Window::run(helper.window_settings())
}
