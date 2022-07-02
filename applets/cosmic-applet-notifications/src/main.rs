use cascade::cascade;
use gtk4::{glib, prelude::*};

mod dbus_service;
mod deref_cell;
mod notification_popover;
use notification_popover::NotificationPopover;
mod notification_list;
mod notification_widget;
use notification_list::NotificationList;
mod notifications;
use notifications::Notifications;

fn main() {
    gtk4::init().unwrap();

    // XXX Implement DBus service somewhere other than applet?
    let notifications = Notifications::new();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(include_bytes!("style.css"));
    gtk4::StyleContext::add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let notification_list = NotificationList::new(&notifications);

    let popover = cascade! {
        gtk4::Popover::new();
        ..set_child(Some(&notification_list));
    };

    let menu_button = cascade! {
        gtk4::MenuButton::new();
        ..set_icon_name("user-invisible-symbolic"); // TODO
        ..set_popover(Some(&popover));
    };

    // XXX show in correct place
    cascade! {
        NotificationPopover::new(&notifications);
        ..set_parent(&menu_button);
    };

    gtk4::Window::builder()
        .decorated(false)
        .child(&menu_button)
        .resizable(false)
        .width_request(1)
        .height_request(1)
        .css_classes(vec!["root_window".to_string()])
        .build()
        .show();

    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();
}
