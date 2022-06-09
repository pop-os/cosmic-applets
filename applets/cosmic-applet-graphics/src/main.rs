// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(unused_parens, clippy::double_parens)] // needed for a quirk in the view! macro

#[macro_use]
extern crate relm4_macros;

pub mod dbus;
pub mod graphics;
pub mod mode_box;

use self::{dbus::PowerDaemonProxy, graphics::Graphics, mode_box::ModeSelection};
use gtk4::{
    gdk::Display,
    gio::ApplicationFlags,
    glib::{self, clone, MainContext, PRIORITY_DEFAULT},
    prelude::*,
    Align, CssProvider, Label, ListBox, ListBoxRow, Orientation, Overlay, Separator, Spinner,
    StyleContext, STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;
use cosmic_panel_config::config::{CosmicPanelConfig, XdgWrapperConfig};

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
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(include_bytes!("style.css"));
    gtk4::StyleContext::add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = gtk4::ApplicationWindow::builder()
        .application(application)
        .title("COSMIC Graphics Applet")
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
    let image = gtk4::Image::from_icon_name("input-gaming");
    image.add_css_class("panel_icon");
    image.set_pixel_size(config.get_applet_icon_size().try_into().unwrap());
    button.set_child(Some(&image));
        let current_graphics = RT
        .block_on(get_current_graphics())
        .expect("failed to connect to system76-power");
    view! {
        icon_box = gtk4::Box {
            set_orientation: Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "icon_box",
        }
    }

    let (tx, rx) = MainContext::channel::<bool>(PRIORITY_DEFAULT);

    view! {
        main_overlay = Overlay {
            add_overlay: loading_box = &gtk4::Box {
                append: loading_explain_box = &gtk4::Box {
                    set_orientation: Orientation::Vertical,
                    set_halign: Align::Center,
                    set_valign: Align::Center,
                    append: loading_spinner = &Spinner {
                        set_halign: Align::Center,
                    },
                    append: loading_explain = &Label {
                        set_label: "Please wait while your graphics mode is set...",
                        set_halign: Align::Center,
                    },
                },
                set_halign: Align::Center,
                set_valign: Align::Center,
                set_hexpand: true,
                set_vexpand: true,
                set_visible: false,
                add_css_class: "loading-overlay",
            },
            set_child: main_box = Some(&gtk4::Box) {
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
                        connect_toggled: clone!(@strong tx => move |_| {
                            tx.send(true).expect("failed to send to main context");
                            let tx = tx.clone();
                            RT.spawn(async move {
                                set_graphics(Graphics::Integrated).await.expect("failed to set graphics mode");
                                tx.send(false).expect("failed to send to main context");
                            });
                        })
                    },
                    append: nvidia_selector = &ModeSelection {
                        set_title: "NVIDIA Graphics",
                        set_group: Some(&integrated_selector),
                        set_active: (current_graphics == Graphics::Nvidia),
                        connect_toggled: clone!(@strong tx => move |_| {
                            tx.send(true).expect("failed to send to main context");
                            let tx = tx.clone();
                            RT.spawn(async move {
                                set_graphics(Graphics::Nvidia).await.expect("failed to set graphics mode");
                                tx.send(false).expect("failed to send to main context");
                            });
                        })
                    },
                    append: hybrid_selector = &ModeSelection {
                        set_title: "Hybrid Graphics",
                        set_description: "Requires Restart.",
                        set_group: Some(&integrated_selector),
                        set_active: (current_graphics == Graphics::Hybrid),
                        connect_toggled: clone!(@strong tx => move |_| {
                            tx.send(true).expect("failed to send to main context");
                            let tx = tx.clone();
                            RT.spawn(async move {
                                set_graphics(Graphics::Hybrid).await.expect("failed to set graphics mode");
                                tx.send(false).expect("failed to send to main context");
                            });
                        })
                    },
                    append: compute_selector = &ModeSelection {
                        set_title: "Compute Graphics",
                        set_description: "Disables external displays. Requires Restart.",
                        set_group: Some(&integrated_selector),
                        set_active: (current_graphics == Graphics::Compute),
                        connect_toggled: clone!(@strong tx => move |_| {
                            tx.send(true).expect("failed to send to main context");
                            let tx = tx.clone();
                            RT.spawn(async move {
                                set_graphics(Graphics::Compute).await.expect("failed to set graphics mode");
                                tx.send(false).expect("failed to send to main context");
                            });
                        })
                    },
                }
            }
        }
    }
    rx.attach(
        None,
        clone!(@weak loading_box, @weak loading_spinner => @default-return Continue(true), move |val| {
            loading_box.set_visible(val);
            loading_spinner.set_spinning(val);
            Continue(true)
        }),
    );
    popover.set_child(Some(&main_overlay));

    icon_box.append(&button);
    icon_box.append(&popover);
    window.set_child(Some(&icon_box));

    window.show();
}
