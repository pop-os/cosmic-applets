use gtk4::{gdk, glib, prelude::*};

mod deref_cell;
mod mpris;
mod status_area;
mod status_menu;
mod time_button;
mod window;
mod x;

fn main() {
    gtk4::init().unwrap();
    let main_context = glib::MainContext::default();
    let _acquire_guard = main_context.acquire().unwrap();

    let display = gdk::Display::default().unwrap();
    let monitors = display.monitors().unwrap();

    for i in 0..monitors.n_items() {
        let monitor = monitors
            .item(i)
            .unwrap()
            .downcast::<gdk::Monitor>()
            .unwrap();
        window::PanelWindow::new(monitor).show();
    }

    monitors.connect_items_changed(|monitors, position, _removed, added| {
        for i in position..position + added {
            let monitor = monitors
                .item(i)
                .unwrap()
                .downcast::<gdk::Monitor>()
                .unwrap();
            window::PanelWindow::new(monitor).show();
        }
    });

    glib::MainLoop::new(None, false).run();
}
