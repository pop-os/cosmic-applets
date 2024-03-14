#[rustfmt::skip]
mod backlight;
mod app;
mod config;
mod dgpu;
mod localize;
mod power_daemon;
mod upower;
mod upower_device;
mod upower_kbdbacklight;

use localize::localize;

pub fn run() -> cosmic::iced::Result {
    localize();
    app::run()
}
