use gtk4::{gdk, glib, prelude::*};

mod window;
mod x;

fn main() {
    gtk4::init().unwrap();

    let display = gdk::Display::default().unwrap();
    let monitors = display.monitors().unwrap();

    for i in 0..monitors.n_items() {
        let monitor = monitors
            .item(i)
            .unwrap()
            .downcast::<gdk::Monitor>()
            .unwrap();
        window::window(monitor);
    }

    monitors.connect_items_changed(|monitors, position, _removed, added| {
        for i in position..position + added {
            let monitor = monitors
                .item(i)
                .unwrap()
                .downcast::<gdk::Monitor>()
                .unwrap();
            window::window(monitor);
        }
    });

    glib::MainLoop::new(None, false).run();
}
