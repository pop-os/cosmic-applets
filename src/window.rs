use cascade::cascade;
use glib::clone;
use gtk4::{gdk, glib, pango, prelude::*, subclass::prelude::*};
use std::cell::Cell;

use crate::application::PanelApp;
use crate::deref_cell::DerefCell;
use crate::status_area::StatusArea;
use crate::time_button::TimeButton;
use crate::x;

const BOTTOM: bool = false;

fn button(text: &str) -> gtk4::Button {
    let label = cascade! {
        gtk4::Label::new(Some(text));
        ..set_attributes(Some(&cascade! {
            pango::AttrList::new();
            ..insert(pango::Attribute::new_weight(pango::Weight::Bold));
        }));
    };

    cascade! {
        gtk4::Button::new();
        ..set_has_frame(false);
        ..set_child(Some(&label));
    }
}

#[derive(Default)]
pub struct PanelWindowInner {
    size: Cell<Option<(i32, i32)>>,
    monitor: DerefCell<gdk::Monitor>,
}

#[glib::object_subclass]
impl ObjectSubclass for PanelWindowInner {
    const NAME: &'static str = "S76PanelWindow";
    type ParentType = gtk4::ApplicationWindow;
    type Type = PanelWindow;
}

impl ObjectImpl for PanelWindowInner {
    fn constructed(&self, obj: &PanelWindow) {
        let box_ = cascade! {
            gtk4::CenterBox::new();
            ..set_start_widget(Some(&cascade! {
                gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                ..append(&button("Workspaces"));
                ..append(&button("Applications"));
            }));
            ..set_center_widget(Some(&TimeButton::new()));
            ..set_end_widget(Some(&StatusArea::new()));
        };

        cascade! {
            obj;
            ..set_decorated(false);
            ..set_child(Some(&box_));
        };
    }
}

impl WidgetImpl for PanelWindowInner {
    fn realize(&self, obj: &PanelWindow) {
        self.parent_realize(obj);

        let surface = obj.surface().unwrap();
        surface.connect_layout(clone!(@weak obj => move |_surface, width, height| {
            let size = Some((width, height));
            if obj.inner().size.replace(size) != size {
                obj.monitor_geometry_changed();
            }
        }));
    }

    fn show(&self, obj: &PanelWindow) {
        self.parent_show(obj);

        if let Some((display, surface)) = x::get_window_x11(obj) {
            unsafe {
                surface.set_skip_pager_hint(true);
                surface.set_skip_taskbar_hint(true);
                x::wm_state_add(&display, &surface, "_NET_WM_STATE_ABOVE");
                x::wm_state_add(&display, &surface, "_NET_WM_STATE_STICKY");
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
                    "_NET_WM_WINDOW_TYPE",
                    x::PropMode::Replace,
                    &[x::Atom::new(&display, "_NET_WM_WINDOW_TYPE_DOCK").unwrap()],
                );
            }
        }

        self.monitor
            .connect_geometry_notify(clone!(@strong obj => move |_| {
                obj.monitor_geometry_changed();
            }));
        obj.monitor_geometry_changed();
    }
}

impl WindowImpl for PanelWindowInner {}
impl ApplicationWindowImpl for PanelWindowInner {}

glib::wrapper! {
    pub struct PanelWindow(ObjectSubclass<PanelWindowInner>)
        @extends gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl PanelWindow {
    pub fn new(app: &PanelApp, monitor: gdk::Monitor) -> Self {
        let obj = glib::Object::new::<Self>(&[]).unwrap();

        monitor.connect_invalidate(clone!(@weak obj => move |_| obj.close()));

        obj.set_size_request(monitor.geometry().width, 0);
        obj.inner().monitor.set(monitor);

        app.add_window(&obj);

        let notifications = app.notifications().clone();
        app.notifications()
            .connect_notification_recieved(clone!(@weak obj => move |id| {
                let notification = notifications.get(id);
                println!(
                    "{:?}",
                    notification
                );
            }));

        obj
    }

    fn inner(&self) -> &PanelWindowInner {
        PanelWindowInner::from_instance(self)
    }

    fn monitor_geometry_changed(&self) {
        let geometry = self.inner().monitor.geometry();
        self.set_size_request(geometry.width, 0);

        let height = if let Some((_width, height)) = self.inner().size.get() {
            height as x::c_ulong
        } else {
            return;
        };

        if let Some((display, surface)) = x::get_window_x11(self) {
            let start_x = geometry.x as x::c_ulong;
            let end_x = start_x + geometry.width as x::c_ulong - 1;

            unsafe {
                let y = if BOTTOM {
                    geometry.height as x::c_int - height as x::c_int
                } else {
                    0
                };

                x::set_position(&display, &surface, start_x as _, y);

                let strut = if BOTTOM {
                    [0, 0, 0, height, 0, 0, 0, 0, 0, 0, start_x, end_x]
                } else {
                    [0, 0, height, 0, 0, 0, 0, 0, start_x, end_x, 0, 0]
                };

                x::change_property(
                    &display,
                    &surface,
                    "_NET_WM_STRUT_PARTIAL",
                    x::PropMode::Replace,
                    &strut,
                );
            }
        }
    }
}
