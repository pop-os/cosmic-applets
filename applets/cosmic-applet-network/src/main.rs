// SPDX-License-Identifier: GPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

pub mod task;
pub mod ui;
pub mod widgets;

use cosmic_panel_config::config::CosmicPanelConfig;
use gtk4::{gio::ApplicationFlags, glib, prelude::*, Orientation, Separator};
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to build tokio runtime"));

fn main() {
    let application = gtk4::Application::new(
        Some("com.system76.cosmic.applets.network"),
        ApplicationFlags::default(),
    );
    application.connect_activate(build_ui);
    application.run();
}

fn build_ui(application: &gtk4::Application) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(include_bytes!("style.css"));
    gtk4::StyleContext::add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = gtk4::ApplicationWindow::builder()
        .application(application)
        .title("COSMIC Network Applet")
        .decorated(false)
        .resizable(false)
        .width_request(1)
        .height_request(1)
        .css_classes(vec!["root_window".to_string()])
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

    let config = CosmicPanelConfig::load_from_env().unwrap_or_default();
    let popover = gtk4::builders::PopoverBuilder::new()
        .autohide(true)
        .has_arrow(false)
        .build();

    let button = gtk4::Button::new();
    button.add_css_class("panel_icon");
    button.connect_clicked(glib::clone!(@weak popover => move |_| {
        popover.show();
    }));

    // TODO cleanup
    let image = gtk4::Image::from_icon_name("preferences-system-network");
    image.add_css_class("panel_icon");
    image.set_pixel_size(config.get_applet_icon_size().try_into().unwrap());
    button.set_child(Some(&image));

    view! {
        icon_box = gtk4::Box {
            set_orientation: Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "icon_box",
        }
    }

    popover.set_child(Some(&main_box));

    icon_box.append(&button);
    icon_box.append(&popover);

    ui::current_networks::add_current_networks(&main_box, &image);
    main_box.append(&Separator::new(Orientation::Horizontal));
    ui::toggles::add_toggles(&main_box);
    let available_wifi_separator = Separator::new(Orientation::Horizontal);
    main_box.append(&available_wifi_separator);
    available_wifi_separator.hide();
    ui::available_wifi::add_available_wifi(&main_box, available_wifi_separator);
    window.set_child(Some(&icon_box));
    window.show();
}
