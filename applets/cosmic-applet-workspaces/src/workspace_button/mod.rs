mod imp;

use crate::{utils::WorkspaceEvent, workspace_object::WorkspaceObject, TX};
use glib::Object;
use gtk4::{glib, prelude::*, subclass::prelude::*, ToggleButton, Label, Align};

glib::wrapper! {
    pub struct WorkspaceButton(ObjectSubclass<imp::WorkspaceButton>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl WorkspaceButton {
    pub fn new() -> Self {
        let self_ = Object::new(&[]).expect("Failed to create `WorkspaceButton`.");
        let imp = imp::WorkspaceButton::from_instance(&self_);

        let tb = ToggleButton::with_label("");
        self_.append(&tb);
        self_.set_hexpand(true);
        imp.button.replace(tb);

        self_.connect_parent_notify(|self_| {
            if let Some(parent) = self_.parent() {
                parent.set_hexpand(true);
            }
        });

        self_
    }

    pub fn set_workspace_object(&self, obj: &WorkspaceObject) {
        let imp = imp::WorkspaceButton::from_instance(&self);
        let old_button = imp.button.take();
        self.remove(&old_button);

        let id = obj.id();
        let new_button = ToggleButton::new();
        new_button.set_hexpand(true);

        let label = Label::new(Some(&id));
        label.set_halign(Align::Center);
        new_button.set_child(Some(&label));
        
        if obj.active() == 1 {
            new_button.add_css_class("alert");
        } else if obj.active() == 0 {
            new_button.add_css_class("active");
        } else {
            new_button.add_css_class("inactive");
        }

        self.append(&new_button);
        new_button.connect_clicked(move |_| {
            let id_clone = id.clone();
            let _ = TX.get().unwrap().send(WorkspaceEvent::Activate(id_clone));
        });

        imp.button.replace(new_button);
    }
}
