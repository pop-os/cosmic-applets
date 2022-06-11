// SPDX-License-Identifier: GPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

pub mod session_manager;
pub mod ui;

use cosmic_panel_config::config::{CosmicPanelConfig, XdgWrapperConfig};
use gtk4::{gio::ApplicationFlags, glib, prelude::*, Align, Button, Label, Orientation, Separator};
use once_cell::sync::Lazy;
use std::process::Command;
use tokio::runtime::Runtime;

static RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to build tokio runtime"));

fn main() {
    let application = gtk4::Application::new(
        Some("com.system76.cosmic.applets.power"),
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
        .title("COSMIC Power Applet")
        .decorated(false)
        .resizable(false)
        .width_request(1)
        .height_request(1)
        .css_classes(vec!["root_window".to_string()])
        .build();

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
    // TODO adjust battery icon based on charge
    let image = gtk4::Image::from_icon_name("battery-full-symbolic");
    image.add_css_class("panel_icon");
    image.set_pixel_size(config.get_applet_icon_size().try_into().unwrap());
    view! {
        icon_box = gtk4::Box {
            set_orientation: Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "icon_box",
        }
    }
    button.set_child(Some(&image));

    let session_section = ui::session::build();
    let system_section = ui::system::build();
    view! {
        main_box = gtk4::Box {
            set_orientation: Orientation::Vertical,
            set_spacing: 10,
            set_margin_top: 20,
            set_margin_bottom: 20,
            set_margin_start: 24,
            set_margin_end: 24,
            append: settings_button = &Button {
                set_child = Some(&Label) {
                    set_label: "Settings...",
                    set_halign: Align::Start,
                    set_hexpand: true
                },
                connect_clicked => move |_| {
                    let _ = Command::new("cosmic-settings").spawn();
                }
            },
            append = &Separator {},
            append: &session_section,
            append: second_separator = &Separator {},
            append: &system_section
        }
    }
    popover.set_child(Some(&main_box));

    icon_box.append(&button);
    icon_box.append(&popover);
    window.set_child(Some(&icon_box));
    window.show();
}
