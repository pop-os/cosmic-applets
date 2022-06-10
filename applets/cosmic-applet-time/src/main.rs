use gtk4::{glib, prelude::*};

mod deref_cell;
mod time_button;
use time_button::TimeButton;

fn main() {
    gtk4::init().unwrap();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(include_bytes!("style.css"));
    gtk4::StyleContext::add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let time_button = TimeButton::new();

    gtk4::Window::builder()
        .decorated(false)
        .child(&time_button)
        .resizable(false)
        .width_request(1)
        .height_request(1)
        .css_classes(vec!["root_window".to_string()])
        .build()
        .show();

    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();
}
