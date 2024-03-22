mod config;
mod localize;
mod time;
mod window;

use window::Window;

pub fn run() -> cosmic::iced::Result {
    localize::localize();

    cosmic::applet::run::<Window>(true, ())
}
