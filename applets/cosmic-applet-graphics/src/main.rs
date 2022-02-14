// SPDX-License-Identifier: LGPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

pub mod dbus;
use gtk4::{gio::ApplicationFlags, prelude::*, Orientation};
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to build tokio runtime"));

fn main() {
    let application = gtk4::Application::new(
        Some("com.system76.cosmic.applets.graphics"),
        ApplicationFlags::default(),
    );
    application.connect_activate(build_ui);
    application.run();
}

fn build_ui(application: &gtk4::Application) {
    let window = gtk4::ApplicationWindow::builder()
        .application(application)
        .title("COSMIC Graphics Applet")
        .default_width(400)
        .default_height(300)
        .build();

    view! {
        main_box = gtk4::Box {
            set_orientation: Orientation::Vertical,
            set_spacing: 10,
            set_margin_top: 20,
            set_margin_bottom: 20,
            set_margin_start: 24,
            set_margin_end: 24
        }
    }

    window.show();
}
