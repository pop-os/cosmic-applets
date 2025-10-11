// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let _ = tracing_log::LogTracer::init();

    let Some(applet) = std::env::args().next() else {
        return Ok(());
    };

    let cmd = applet.rsplit_once('/').map_or(applet.as_str(), |s| s.1);

    tracing::info!("Starting `{cmd}` with version {VERSION}");

    match cmd {
        "cosmic-app-list" => cosmic_app_list::run(),
        "cosmic-applet-a11y" => cosmic_applet_a11y::run(),
        "cosmic-applet-audio" => cosmic_applet_audio::run(),
        "cosmic-applet-battery" => cosmic_applet_battery::run(),
        "cosmic-applet-bluetooth" => cosmic_applet_bluetooth::run(),
        "cosmic-applet-minimize" => cosmic_applet_minimize::run(),
        "cosmic-applet-network" => cosmic_applet_network::run(),
        "cosmic-applet-notifications" => cosmic_applet_notifications::run(),
        "cosmic-applet-power" => cosmic_applet_power::run(),
        "cosmic-applet-status-area" => cosmic_applet_status_area::run(),
        "cosmic-applet-tiling" => cosmic_applet_tiling::run(),
        "cosmic-applet-time" => cosmic_applet_time::run(),
        "cosmic-applet-workspaces" => cosmic_applet_workspaces::run(),
        "cosmic-applet-input-sources" => cosmic_applet_input_sources::run(),
        "cosmic-panel-button" => cosmic_panel_button::run(),
        _ => Ok(()),
    }
}
