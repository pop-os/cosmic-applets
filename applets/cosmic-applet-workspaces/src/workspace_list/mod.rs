// SPDX-License-Identifier: MPL-2.0-only

use crate::utils::Activate;
use crate::workspace_button::WorkspaceButton;
use crate::workspace_object::WorkspaceObject;
use cascade::cascade;
use cosmic_panel_config::config::CosmicPanelConfig;
use gtk4::ListView;
use gtk4::Orientation;
use gtk4::SignalListItemFactory;
use gtk4::{gio, glib, prelude::*, subclass::prelude::*};
use tokio::sync::mpsc::Sender;

mod imp;

glib::wrapper! {
    pub struct WorkspaceList(ObjectSubclass<imp::WorkspaceList>)
        @extends gtk4::Widget, gtk4::Box,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl WorkspaceList {
    pub fn new(config: CosmicPanelConfig) -> Self {
        let self_: WorkspaceList = glib::Object::new(&[]).expect("Failed to create WorkspaceList");
        let imp = imp::WorkspaceList::from_instance(&self_);
        imp.config.set(config).unwrap();
        self_.layout();
        //dnd behavior is different for each type, as well as the data in the model
        self_.setup_model();
        self_.setup_factory();
        self_
    }

    pub fn model(&self) -> &gio::ListStore {
        // Get state
        let imp = imp::WorkspaceList::from_instance(self);
        imp.model.get().expect("Could not get model")
    }

    fn layout(&self) {
        let imp = imp::WorkspaceList::from_instance(self);
        let list_view = cascade! {
            ListView::default();
            ..set_orientation(Orientation::Horizontal);
            ..add_css_class("transparent");
        };
        self.append(&list_view);
        imp.list_view.set(list_view).unwrap();
    }

    fn setup_model(&self) {
        let imp = imp::WorkspaceList::from_instance(self);
        let model = gio::ListStore::new(WorkspaceObject::static_type());

        let selection_model = gtk4::NoSelection::new(Some(&model));

        // Wrap model with selection and pass it to the list view
        let list_view = imp.list_view.get().unwrap();
        list_view.set_model(Some(&selection_model));
        imp.model.set(model).expect("Could not set model");
    }

    fn setup_factory(&self) {
        let imp = imp::WorkspaceList::from_instance(self);
        let factory = SignalListItemFactory::new();
        let model = imp.model.get().expect("Failed to get saved app model.");
        let icon_size = imp.config.get().unwrap().get_applet_icon_size();
        factory.connect_setup(glib::clone!(@weak model => move |_, list_item| {
            let workspace_button = WorkspaceButton::new();
            list_item.set_child(Some(&workspace_button));
        }));
        factory.connect_bind(|_, list_item| {
            let workspace_object = list_item
                .item()
                .expect("The item has to exist.")
                .downcast::<WorkspaceObject>()
                .expect("The item has to be a `DockObject`");
            let workspace_button = list_item
                .child()
                .expect("The list item child needs to exist.")
                .downcast::<WorkspaceButton>()
                .expect("The list item type needs to be `DockItem`");
            workspace_button.set_workspace_object(&workspace_object);
        });
        // Set the factory of the list view
        imp.list_view.get().unwrap().set_factory(Some(&factory));
    }
}
