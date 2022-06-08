// SPDX-License-Identifier: MPL-2.0-only

use gtk4::{glib, subclass::prelude::*};

mod imp;

glib::wrapper! {
    pub struct WorkspaceObject(ObjectSubclass<imp::WorkspaceObject>);
}

impl WorkspaceObject {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }

    pub fn from_id_active(id: u32, active: bool) -> Self {
        glib::Object::new(&[("id", &id), ("active", &active)]).unwrap()
    }

    pub fn id(&self) -> u32 {
        imp::WorkspaceObject::from_instance(&self).id.get()
    } 

    pub fn active(&self) -> bool {
        imp::WorkspaceObject::from_instance(&self).active.get()
    } 
}

#[derive(Clone, Debug, Default, glib::Boxed)]
#[boxed_type(name = "BoxedWorkspaceObject")]
pub struct BoxedWorkspaceObject(pub Option<WorkspaceObject>);
