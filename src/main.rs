use gtk4::{glib, prelude::*};

mod application;
mod dbus_service;
mod deref_cell;
mod mpris;
mod mpris_player;
mod notification_list;
mod notification_popover;
mod notification_widget;
mod notifications;
mod popover_container;
mod status_area;
mod status_menu;
mod status_notifier_watcher;
mod time_button;
mod window;

use application::PanelApp;

fn main() {
    glib::MainContext::default().with_thread_default(|| PanelApp::new().run());
}
