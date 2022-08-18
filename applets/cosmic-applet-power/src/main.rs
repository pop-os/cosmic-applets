// SPDX-License-Identifier: GPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

pub mod cosmic_session;
pub mod session_manager;
pub mod ui;

use gtk4::{gio::ApplicationFlags, prelude::*, Align, Button, Label, Orientation, Separator};
use once_cell::sync::Lazy;
use std::process::Command;
use tokio::runtime::Runtime;

static RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to build tokio runtime"));

fn main() {
    let _monitors = libcosmic::init();

    let application = gtk4::Application::new(None, ApplicationFlags::default());
    application.connect_activate(build_ui);
    application.run();
}

fn build_ui(application: &gtk4::Application) {
    view! {
        window = libcosmic_applet::AppletWindow {
            set_title: Some("COSMIC Power Applet"),
            set_application: Some(application),
            // TODO adjust battery icon based on charge
            #[wrap(Some)]
            set_child = &libcosmic_applet::AppletButton {
                set_button_icon_name: "system-shutdown-symbolic",
                #[wrap(Some)]
                set_popover_child: main_box = &gtk4::Box {
                    set_orientation: Orientation::Vertical,
                    set_spacing: 10,
                    set_margin_top: 20,
                    set_margin_bottom: 20,
                    set_margin_start: 24,
                    set_margin_end: 24,
                    append: settings_button = &Button {
                        #[wrap(Some)]
                        set_child = &Label {
                            set_label: "Settings...",
                            set_halign: Align::Start,
                            set_hexpand: true
                        },
                        connect_clicked => move |_| {
                            let _ = Command::new("cosmic-settings").spawn();
                        }
                    },
                    append = &Separator {},
                    append: &ui::session::build(),
                    append: second_separator = &Separator {},
                    append: &ui::system::build(),
                }
            }
        }
    }
    window.show();
}
