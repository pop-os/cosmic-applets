// SPDX-License-Identifier: MPL-2.0-only
mod app;
mod config;
mod localize;
mod wayland_handler;
mod wayland_subscription;

use localize::localize;

pub fn run() -> cosmic::iced::Result {
    localize();

    app::run()
}
