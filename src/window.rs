use cascade::cascade;
use glib::clone;
use gtk4::{gdk, glib, prelude::*};

use crate::x;

pub fn window(monitor: gdk::Monitor) -> gtk4::Window {
    let box_ = cascade! {
        gtk4::CenterBox::new();
        ..set_start_widget(Some(&cascade! {
            gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            ..append(&gtk4::Button::with_label("Workspaces"));
            ..append(&gtk4::Button::with_label("Applications"));
        }));
        ..set_center_widget(Some(&cascade! {
            gtk4::MenuButton::new();
            ..set_label("Jan 1 00:00 AM");
            ..set_popover(Some(&cascade! {
                gtk4::Popover::new();
                ..set_child(Some(&cascade! {
                    gtk4::Calendar::new();
                }));
            }));
        }));
    };

    let window = cascade! {
        gtk4::Window::new();
        ..set_decorated(false);
        //..set_keep_above(true);
        //..stick();
        ..set_child(Some(&box_));
        ..connect_realize(|window| {
        });
        ..show();
    };

    if let Some((display, surface)) = x::get_window_x11(&window) {
        unsafe {
            surface.set_skip_pager_hint(true);
            surface.set_skip_taskbar_hint(true);
            x::change_property(
                &display,
                &surface,
                "_NET_WM_STATE",
                x::PropMode::Append,
                &[
                    x::Atom::new(&display, "_NET_WM_STATE_ABOVE").unwrap(),
                    x::Atom::new(&display, "_NET_WM_STATE_STICKY").unwrap(),
                ],
            ); // XXX not working?
            x::change_property(
                &display,
                &surface,
                "_NET_WM_ALLOWED_ACTIONS",
                x::PropMode::Replace,
                &[
                    x::Atom::new(&display, "_NET_WM_ACTION_CHANGE_DESKTOP").unwrap(),
                    x::Atom::new(&display, "_NET_WM_ACTION_ABOVE").unwrap(),
                    x::Atom::new(&display, "_NET_WM_ACTION_BELOW").unwrap(),
                ],
            );
            x::change_property(
                &display,
                &surface,
                "_NET_WM_STRUT",
                x::PropMode::Replace,
                &[0, 0, 32 as x::c_ulong, 0],
            );
            x::change_property(
                &display,
                &surface,
                "_NET_WM_WINDOW_TYPE",
                x::PropMode::Replace,
                &[x::Atom::new(&display, "_NET_WM_WINDOW_TYPE_DOCK").unwrap()],
            );
        }
    }

    let gdk::Rectangle {
        x,
        y,
        width,
        height,
    } = monitor.geometry();
    window.set_size_request(width, 0);
    monitor.connect_geometry_notify(clone!(@strong window => move |monitor| {
        let gdk::Rectangle { x, y, width, height } = monitor.geometry();
        window.set_size_request(width, 0);
    }));
    monitor.connect_invalidate(clone!(@strong window => move |_| window.close()));

    window
}
