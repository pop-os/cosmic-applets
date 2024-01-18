mod components;
#[rustfmt::skip]
mod config;
mod localize;
mod wayland;
mod wayland_subscription;

use config::APP_ID;
use log::info;

use localize::localize;

use crate::components::app;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> cosmic::iced::Result {
    // Initialize logger
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();

    info!("Starting audio applet with version {VERSION}");
    info!("Iced Workspaces Applet ({VERSION})");

    // Prepare i18n
    localize();

    app::run()
}
