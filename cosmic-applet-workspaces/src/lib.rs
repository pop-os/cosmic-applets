// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod components;
#[rustfmt::skip]
mod config;
mod localize;
mod wayland;
mod wayland_subscription;

use localize::localize;

use crate::components::app;

pub fn run() -> cosmic::iced::Result {
    localize();

    app::run()
}
