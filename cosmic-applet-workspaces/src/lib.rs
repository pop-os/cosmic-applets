mod components;
#[rustfmt::skip]
mod config;
mod localize;
mod wayland;
mod wayland_subscription;

use localize::localize;

use crate::components::app;

pub fn run() -> cosmic::iced::Result {
    localize();

    app::run()
}
