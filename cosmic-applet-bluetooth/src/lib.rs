// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod bluetooth;
mod config;
mod localize;

use crate::localize::localize;

#[inline]
pub fn run() -> cosmic::iced::Result {
    localize();
    app::run()
}
