mod imp;

use glib::Object;
use gtk4::{glib, prelude::*, subclass::prelude::*, ToggleButton};
use tokio::sync::mpsc;
use crate::{Event, workspace_object::WorkspaceObject};

glib::wrapper! {
    pub struct WorkspaceButton(ObjectSubclass<imp::WorkspaceButton>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl WorkspaceButton {
    pub fn new(tx: mpsc::Sender<Event>) -> Self {
        let self_ = Object::new(&[]).expect("Failed to create `WorkspaceButton`.");
        let imp = imp::WorkspaceButton::from_instance(&self_);
        imp.tx.set(tx).unwrap();

        let tb = ToggleButton::with_label("");
        self_.append(&tb);

        imp.button.replace(tb);
        
        self_
    }

    pub fn set_workspace_object(&self, obj: &WorkspaceObject) {
        let imp = imp::WorkspaceButton::from_instance(&self);
        let old_button = imp.button.take();
        self.remove(&old_button);

        let id = obj.id();
        let new_button = ToggleButton::with_label(&format!("{}", id));
        new_button.set_active(obj.active());
        self.append(&new_button);
        new_button.connect_clicked(glib::clone!(@weak imp.tx as tx => move |_| {
            let _ = tx.get().unwrap().send(Event::Activate(id));
        }));

        imp.button.replace(new_button);
    }
}
