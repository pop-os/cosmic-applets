// SPDX-License-Identifier: MPL-2.0-only
mod app;
mod config;
mod localize;
mod wayland_handler;
mod wayland_subscription;

use log::info;

use localize::localize;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> cosmic::iced::Result {
    // Initialize logger
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();
    info!("Iced Workspaces Applet ({VERSION})");
    // Prepare i18n
    localize();

    app::run()
}
