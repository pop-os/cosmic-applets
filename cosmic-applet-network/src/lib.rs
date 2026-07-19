// SPDX-License-Identifier: GPL-3.0-or-later

mod app;
mod config;
mod localize;
mod vpn_auth;

use crate::localize::localize;

pub fn run() -> cosmic::iced::Result {
    localize();
    app::run()
}
