use gtk4::{glib, prelude::*};
use relm4_macros::view;

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
    let _ = gtk4::init();
    adw::init();
    
    // XXX Implement DBus service somewhere other than applet?
    let notifications = Notifications::new();

    let notification_list = NotificationList::new(&notifications);

    view! {
        window = libcosmic_applet::AppletWindow {
            #[wrap(Some)]
            set_child: applet_button = &libcosmic_applet::AppletButton {
                set_button_icon_name: "user-invisible-symbolic", // TODO
                set_popover_child: Some(&notification_list)
            }
        }
    }
    window.show();

    // XXX show in correct place
    let notification_popover = NotificationPopover::new(&notifications);
    notification_popover.set_parent(&applet_button);

    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();
}
