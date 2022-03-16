// SPDX-License-Identifier: LGPL-3.0-or-later

#[macro_use]
extern crate relm4;

mod app;

use relm4::RelmApp;

fn main() {
    RelmApp::<app::App>::new("com.system76.cosmic.applets.audio").run(());
}
