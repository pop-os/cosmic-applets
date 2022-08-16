use cascade::cascade;
use gtk4::{glib, prelude::*};

mod deref_cell;
mod time_button;
use time_button::TimeButton;

fn main() {
    let _ = libcosmic::init();

    cascade! {
        libcosmic_applet::AppletWindow::new();
        ..set_child(Some(&TimeButton::new()));
        ..show();
    };

    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();
}
