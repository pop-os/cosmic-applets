// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod components;
mod subscriptions;

pub fn run() -> cosmic::iced::Result {
    components::app::main()
}
