// SPDX-License-Identifier: MPL-2.0-only

use crate::{apps_container::AppsContainer, fl, AppListEvent};
use cascade::cascade;
use gtk4::{
    gio,
    glib::{self, Object},
    prelude::*,
    subclass::prelude::*,
};

mod imp;

glib::wrapper! {
    pub struct CosmicAppListWindow(ObjectSubclass<imp::CosmicAppListWindow>)
        @extends gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl CosmicAppListWindow {
    pub fn new(app: &gtk4::Application) -> Self {
        let self_: Self =
            Object::new(&[("application", app)]).expect("Failed to create `CosmicAppListWindow`.");
        let imp = imp::CosmicAppListWindow::from_instance(&self_);

        cascade! {
            &self_;
            ..set_width_request(1);
            ..set_height_request(1);
            ..set_decorated(false);
            ..set_resizable(false);
            ..set_title(Some(&fl!("cosmic-app-list")));
            ..add_css_class("transparent");
        };
        let app_list = AppsContainer::new();
        self_.set_child(Some(&app_list));
        imp.inner.set(app_list).unwrap();

        self_.setup_shortcuts();

        self_
    }

    pub fn apps_container(&self) -> &AppsContainer {
        imp::CosmicAppListWindow::from_instance(&self)
            .inner
            .get()
            .unwrap()
    }

    fn setup_shortcuts(&self) {
        let window = self.clone().upcast::<gtk4::Window>();
        let action_quit = gio::SimpleAction::new("quit", None);
        action_quit.connect_activate(glib::clone!(@weak window => move |_, _| {
            window.close();
            if let Some(a) = window.application() { a.quit() }
            std::process::exit(0);
        }));
        self.add_action(&action_quit);
    }
}
