// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::{env, process};

mod components;
mod subscriptions;

pub mod status_notifier_watcher;
mod unique_names;

pub fn run() -> cosmic::iced::Result {
    if let Some(arg) = env::args().nth(1) {
        if arg == "--status-notifier-watcher" {
            status_notifier_watcher::run()
        } else {
            tracing::error!("Invalid argument `{arg}` for status-area applet`");
            process::exit(1);
        }
    } else {
        components::app::main()
    }
}
