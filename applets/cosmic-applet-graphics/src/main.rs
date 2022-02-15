// SPDX-License-Identifier: LGPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

pub mod dbus;
pub mod graphics;
pub mod mode_box;
pub mod profile;

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

async fn get_current_graphics() -> zbus::Result<graphics::Graphics> {
    let connection = zbus::Connection::system().await?;
    let proxy = dbus::PowerDaemonProxy::new(&connection).await?;
    graphics::get_current_graphics(&proxy).await
}

async fn set_graphics(graphics_mode: graphics::Graphics) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    let proxy = dbus::PowerDaemonProxy::new(&connection).await?;
    graphics::set_graphics(&proxy, graphics_mode).await
}

fn build_ui(application: &gtk4::Application) {
    let window = gtk4::ApplicationWindow::builder()
        .application(application)
        .title("COSMIC Graphics Applet")
        .default_width(400)
        .default_height(300)
        .build();
    let current_graphics = RT
        .block_on(get_current_graphics())
        .expect("failed to connect to system76-power");
    view! {
        main_box = gtk4::Box {
            set_orientation: Orientation::Vertical,
            set_spacing: 10,
            set_margin_top: 20,
            set_margin_bottom: 20,
            set_margin_start: 24,
            set_margin_end: 24,
            append: mode_label = &gtk4::Label {
                set_text: "Graphics Mode"
            },
            append: separator = &gtk4::Separator {
                set_orientation: Orientation::Horizontal
            }
        }
    }

    window.show();
}
