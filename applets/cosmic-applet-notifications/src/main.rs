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

    let notification_list = NotificationList::new(&notifications);

    let window = cascade! {
        libcosmic_applet::Applet::new();
        ..set_button_icon_name("user-invisible-symbolic"); // TODO
        ..set_popover_child(Some(&notification_list));
        ..show();
    };

    // XXX show in correct place
    cascade! {
        NotificationPopover::new(&notifications);
        ..set_parent(&window.child().unwrap()); // XXX better way?
    };

    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();
}
