use gtk4::prelude::*;

mod application;
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
mod x;

use application::PanelApp;

fn main() {
    PanelApp::new().run();
}
