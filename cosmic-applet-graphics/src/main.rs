mod dbus;
mod graphics;
mod localize;
mod window;

use window::*;

pub fn main() -> cosmic::iced::Result {
    localize::localize();

    cosmic::applet::run::<Window>(true, ())
}
