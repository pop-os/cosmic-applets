// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

pub use cosmic_applets_config::time as config;

mod localize;
mod window;

use window::Window;

pub fn run() -> cosmic::iced::Result {
    localize::localize();

    cosmic::applet::run::<Window>(())
}
