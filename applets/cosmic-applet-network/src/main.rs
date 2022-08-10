// SPDX-License-Identifier: GPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

pub mod task;
pub mod ui;
pub mod widgets;

use gtk4::{glib, prelude::*, Orientation, Separator};
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to build tokio runtime"));

fn main() {
    let _ = gtk4::init();
    adw::init();
    
    view! {
        window = libcosmic_applet::AppletWindow {
            set_title: Some("COSMIC Network Applet"),
            #[wrap(Some)]
            set_child: button = &libcosmic_applet::AppletButton {
                set_button_icon_name: "preferences-system-network",
                #[wrap(Some)]
                set_popover_child: main_box = &gtk4::Box {
                    set_orientation: Orientation::Vertical,
                    set_spacing: 10,
                    set_margin_top: 20,
                    set_margin_bottom: 20,
                    set_margin_start: 24,
                    set_margin_end: 24
                }
            }
        }
    }

    ui::current_networks::add_current_networks(&main_box, &button);
    main_box.append(&Separator::new(Orientation::Horizontal));
    ui::toggles::add_toggles(&main_box);
    let available_wifi_separator = Separator::new(Orientation::Horizontal);
    main_box.append(&available_wifi_separator);
    available_wifi_separator.hide();
    ui::available_wifi::add_available_wifi(&main_box, available_wifi_separator);
    window.show();

    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();
}
