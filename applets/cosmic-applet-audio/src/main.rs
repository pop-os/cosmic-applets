// SPDX-License-Identifier: LGPL-3.0-or-later

#[macro_use]
extern crate relm4;

mod app;

use relm4::RelmApp;

fn main() {
    figure_out_apps();
    RelmApp::<app::App>::new("com.system76.cosmic.applets.audio").run(());
}

fn figure_out_apps() {
    use pulsectl::controllers::{AppControl, SinkController};

    let mut sink = SinkController::create().unwrap();
    for app in sink.list_applications().unwrap() {
        println!("{:#?}", app);
    }
}
