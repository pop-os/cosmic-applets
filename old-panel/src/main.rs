use gtk4::{glib, prelude::*};

mod application;
mod deref_cell;
mod mpris;
mod mpris_player;
mod popover_container;
mod time_button;
mod window;

use application::PanelApp;

fn main() {
    glib::MainContext::default()
        .with_thread_default(|| PanelApp::new().run())
        .unwrap();
}
