// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> cosmic::iced::Result {
    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            EnvFilter::new(format!("warn,{}=debug", env!("CARGO_CRATE_NAME")))
        } else {
            EnvFilter::new("warn")
        }
    });

    if let Ok(journal_layer) = tracing_journald::layer() {
        tracing_subscriber::registry()
            .with(journal_layer)
            .with(filter_layer.clone())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(fmt_layer)
            .with(filter_layer)
            .init();
    }

    let _ = tracing_log::LogTracer::init();

    tracing::info!("Starting cosmic-app-list with version {VERSION}");

    cosmic_app_list::run()
}
