// SPDX-License-Identifier: GPL-3.0-or-later

mod app;
mod config;
mod localize;
mod network_manager;

use log::info;

use crate::config::{APP_ID, PROFILE, VERSION};
use crate::localize::localize;

fn main() -> cosmic::iced::Result {
    // Initialize logger
    pretty_env_logger::init();
    info!("Iced Workspaces Applet ({})", APP_ID);
    info!("Version: {} ({})", VERSION, PROFILE);

    // Prepare i18n
    localize();

    app::run()
}
