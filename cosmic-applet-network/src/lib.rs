// SPDX-License-Identifier: GPL-3.0-or-later

mod app;
mod config;
mod graph;
mod localize;
mod stats;

use crate::localize::localize;

pub fn run() -> cosmic::iced::Result {
    localize();
    app::run()
}
