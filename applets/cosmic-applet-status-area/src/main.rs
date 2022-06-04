use gtk4::{glib, prelude::*};

mod dbus_service;
mod deref_cell;
mod status_area;
mod status_menu;
mod status_notifier_watcher;

use status_area::StatusArea;

fn main() {
    gtk4::init().unwrap();

    // XXX Implement DBus service somewhere other than applet?
    glib::MainContext::default().spawn_local(status_notifier_watcher::start());

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(include_bytes!("style.css"));
    gtk4::StyleContext::add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let status_area = StatusArea::new();
    gtk4::Window::builder()
        .decorated(false)
        .child(&status_area)
        .resizable(false)
        .width_request(1)
        .height_request(1)
        .css_classes(vec!["root_window".to_string()])
        .build()
        .show();

    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();
}
