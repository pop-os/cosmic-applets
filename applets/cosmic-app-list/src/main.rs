// SPDX-License-Identifier: MPL-2.0-only
mod app;
mod config;
mod localize;
mod toplevel_handler;
mod toplevel_subscription;

use log::info;

use localize::localize;

use crate::config::{APP_ID, VERSION};

fn main() -> cosmic::iced::Result {
    // Initialize logger
    pretty_env_logger::init();
    info!("Iced Workspaces Applet ({})", APP_ID);
    info!("Version: {}", VERSION);
    config::AppListConfig::default().save().unwrap();
    // Prepare i18n
    localize();

    app::run()
}
