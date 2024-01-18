// SPDX-License-Identifier: GPL-3.0-or-later

mod app;
mod bluetooth;
mod config;
mod localize;

use log::info;

use crate::config::{APP_ID, PROFILE, VERSION};
use crate::localize::localize;

fn main() -> cosmic::iced::Result {
    // Initialize logger
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();
    info!("Iced Workspaces Applet ({})", APP_ID);
    info!("Version: {} ({})", VERSION, PROFILE);

    // Prepare i18n
    localize();

    app::run()
}
