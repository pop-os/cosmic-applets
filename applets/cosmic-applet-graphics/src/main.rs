// SPDX-License-Identifier: LGPL-3.0-or-later

#![allow(unused_parens, clippy::double_parens)] // needed for a quirk in the view! macro

#[macro_use]
extern crate relm4_macros;

pub mod dbus;
pub mod graphics;
pub mod mode_box;
pub mod profile;

use self::{dbus::PowerDaemonProxy, graphics::Graphics, mode_box::ModeSelection};
use gtk4::{gio::ApplicationFlags, prelude::*, Label, ListBox, ListBoxRow, Orientation, Separator};
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

async fn get_current_graphics() -> zbus::Result<Graphics> {
    let connection = zbus::Connection::system().await?;
    let proxy = PowerDaemonProxy::new(&connection).await?;
    graphics::get_current_graphics(&proxy).await
}

async fn set_graphics(graphics_mode: Graphics) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    let proxy = PowerDaemonProxy::new(&connection).await?;
    graphics::set_graphics(&proxy, graphics_mode).await
}

fn row_clicked(_: &ListBox, row: &ListBoxRow) {
    let child = row.child().expect("UNEXPECTED: row has no child");
    let selector = child
        .downcast::<ModeSelection>()
        .expect("UNEXPECTED: child is not a mode selector");
    selector.emit_activate();
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
            append: mode_label = &Label {
                set_text: "Graphics Mode"
            },
            append: separator = &Separator {
                set_orientation: Orientation::Horizontal
            },
            append: graphics_modes_list = &ListBox {
                connect_row_activated: row_clicked,
                append: integrated_selector = &ModeSelection {
                    set_title: "Integrated Graphics",
                    set_description: "Disables external displays. Requires Restart.",
                    set_active: (current_graphics == Graphics::Integrated),
                    connect_toggled: |_| {
                        RT.block_on(set_graphics(Graphics::Integrated)).expect("failed to set graphics");
                    }
                },
                append: nvidia_selector = &ModeSelection {
                    set_title: "NVIDIA Graphics",
                    set_group: Some(&integrated_selector),
                    set_active: (current_graphics == Graphics::Nvidia),
                    connect_toggled: |_| {
                        RT.block_on(set_graphics(Graphics::Nvidia)).expect("failed to set graphics");
                    }
                },
                append: hybrid_selector = &ModeSelection {
                    set_title: "Hybrid Graphics",
                    set_description: "Requires Restart.",
                    set_group: Some(&integrated_selector),
                    set_active: (current_graphics == Graphics::Hybrid),
                    connect_toggled: |_| {
                        RT.block_on(set_graphics(Graphics::Hybrid)).expect("failed to set graphics");
                    }
                },
                append: compute_selector = &ModeSelection {
                    set_title: "Compute Graphics",
                    set_description: "Disables external displays. Requires Restart.",
                    set_group: Some(&integrated_selector),
                    set_active: (current_graphics == Graphics::Compute),
                    connect_toggled: |_| {
                        RT.block_on(set_graphics(Graphics::Compute)).expect("failed to set graphics");
                    }
                },
            }
        }
    }
    window.set_child(Some(&main_box));

    window.show();
}
