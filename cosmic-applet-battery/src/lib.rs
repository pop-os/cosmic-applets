// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

#[rustfmt::skip]
mod backlight;
mod app;
mod backend;
mod config;
mod dgpu;
mod localize;
mod upower_device;
mod upower_kbdbacklight;

use localize::localize;

pub fn run() -> cosmic::iced::Result {
    localize();
    app::run()
}
