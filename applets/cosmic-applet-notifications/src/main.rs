use gtk4::{glib, prelude::*, PositionType};
use relm4_macros::view;
use cosmic_panel_config::PanelAnchor;

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
    let _monitors = libcosmic::init();

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
    let position = std::env::var("COSMIC_PANEL_ANCHOR")
    .ok()
    .and_then(|anchor| anchor.parse::<PanelAnchor>().ok())
    .map(|anchor| match anchor {
        PanelAnchor::Left => PositionType::Right,
        PanelAnchor::Right => PositionType::Left,
        PanelAnchor::Top => PositionType::Bottom,
        PanelAnchor::Bottom => PositionType::Top,
    });
    let notification_popover = NotificationPopover::new(&notifications, position);
    notification_popover.set_parent(&applet_button);

    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();
}
