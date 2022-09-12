// SPDX-License-Identifier: MPL-2.0-only

use std::cmp::Ordering;

use crate::utils::WorkspaceEvent;
use crate::wayland::State;
use crate::workspace_button::WorkspaceButton;
use crate::workspace_object::WorkspaceObject;
use crate::TX;
use cascade::cascade;
use cosmic_panel_config::PanelAnchor;
use gtk4::builders::EventControllerScrollBuilder;
use gtk4::EventControllerScrollFlags;
use gtk4::Inhibit;
use gtk4::ListView;
use gtk4::SignalListItemFactory;
use gtk4::{gio, glib, prelude::*, subclass::prelude::*};
use itertools::Itertools;

mod imp;

glib::wrapper! {
    pub struct WorkspaceList(ObjectSubclass<imp::WorkspaceList>)
        @extends gtk4::Widget, gtk4::Box,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl WorkspaceList {
    pub fn new() -> Self {
        let self_: WorkspaceList = glib::Object::new(&[]).expect("Failed to create WorkspaceList");
        let imp = imp::WorkspaceList::from_instance(&self_);
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
        let anchor = std::env::var("COSMIC_PANEL_ANCHOR")
            .ok()
            .and_then(|anchor| anchor.parse::<PanelAnchor>().ok())
            .unwrap_or_default();

        let list_view = cascade! {
            ListView::default();
            ..set_orientation(anchor.into());
            ..add_css_class("transparent");
        };
        self.append(&list_view);

        let flags = EventControllerScrollFlags::BOTH_AXES;

        let scroll_controller = EventControllerScrollBuilder::new()
            .flags(flags.union(EventControllerScrollFlags::DISCRETE))
            .build();

        scroll_controller.connect_scroll(|_, dx, dy| {
            let _ = TX.get().unwrap().send(WorkspaceEvent::Scroll(dx + dy));
            Inhibit::default()
        });

        list_view.add_controller(&scroll_controller);
        imp.list_view.set(list_view).unwrap();
    }

    pub fn set_workspaces(&self, workspaces: State) {
        let imp = imp::WorkspaceList::from_instance(&self);
        let model = imp.model.get().unwrap();

        let model_len = model.n_items();
        let new_results: Vec<glib::Object> = workspaces
            .workspace_list()
            .sorted_by(|a, b| {
                match a.0.len().cmp(&b.0.len()) {
                    Ordering::Less => Ordering::Less,
                    Ordering::Equal => a.0.cmp(&b.0),
                    Ordering::Greater => Ordering::Greater,
                }
            })
            .filter_map(|w| {
                // don't include hidden workspaces
                if w.1 != 2 {
                    Some(WorkspaceObject::from_id_active(w.0, w.1).upcast())
                } else {
                    None
                }
            })
            .collect();
        model.splice(0, model_len, &new_results[..]);
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

        factory.connect_setup(glib::clone!(@weak model => move |_, list_item| {
            let workspace_button = WorkspaceButton::new();
            list_item.set_child(Some(&workspace_button));
        }));
        factory.connect_bind(|_, list_item| {
            let workspace_object = list_item
                .item()
                .expect("The item has to exist.")
                .downcast::<WorkspaceObject>()
                .expect("The item has to be a `WorkspaceObject`");
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
