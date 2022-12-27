mod dbus;
mod graphics;
mod window;

use cosmic::{applet::CosmicAppletHelper, iced::Application};
use window::*;

pub fn main() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    Window::run(helper.window_settings())
}
