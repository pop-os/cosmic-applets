use gtk4::{
    gdk, gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use std::cell::Cell;

use crate::deref_cell::DerefCell;
use crate::notifications::Notifications;
use crate::window;

#[derive(Default)]
pub struct PanelAppInner {
    notifications: DerefCell<Notifications>,
    activated: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for PanelAppInner {
    const NAME: &'static str = "S76CosmicPanelApp";
    type ParentType = gtk4::Application;
    type Type = PanelApp;
}

impl ObjectImpl for PanelAppInner {
    fn constructed(&self, obj: &PanelApp) {
        obj.set_application_id(Some("com.system76.cosmicpanel"));

        self.parent_constructed(obj);

        self.notifications.set(Notifications::new());
    }
}

impl ApplicationImpl for PanelAppInner {
    fn activate(&self, obj: &PanelApp) {
        self.parent_activate(obj);

        if self.activated.get() {
            return;
        }
        self.activated.set(true);

        let display = gdk::Display::default().unwrap();
        let monitors = display.monitors();

        for i in 0..monitors.n_items() {
            obj.add_window_for_monitor(monitors.item(i).unwrap().downcast().unwrap());
        }

        monitors.connect_items_changed(
            clone!(@weak obj => move |monitors, position, _removed, added| {
                for i in position..position + added {
                    obj.add_window_for_monitor(monitors
                        .item(i)
                        .unwrap()
                        .downcast::<gdk::Monitor>()
                        .unwrap());
                }
            }),
        );
    }
}

impl GtkApplicationImpl for PanelAppInner {}

glib::wrapper! {
    pub struct PanelApp(ObjectSubclass<PanelAppInner>)
        @extends gtk4::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl PanelApp {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).unwrap()
    }

    fn inner(&self) -> &PanelAppInner {
        PanelAppInner::from_instance(self)
    }

    fn add_window_for_monitor(&self, monitor: gdk::Monitor) {
        window::create(self, monitor);
    }

    pub fn notifications(&self) -> &Notifications {
        &*self.inner().notifications
    }
}
