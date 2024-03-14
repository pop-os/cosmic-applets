// SPDX-License-Identifier: GPL-3.0-or-later

mod app;
mod bluetooth;
mod config;
mod localize;

use crate::localize::localize;

pub fn run() -> cosmic::iced::Result {
    localize();
    app::run()
}
