// SPDX-License-Identifier: MPL-2.0-only

use std::cell::{RefCell, Cell};

use glib::{ParamFlags, ParamSpec, Value};
use gtk4::gdk::glib::ParamSpecBoolean;
use gtk4::glib::{self, ParamSpecString};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use once_cell::sync::Lazy;

// Object holding the state
#[derive(Default)]
pub struct WorkspaceObject {
    pub(crate) id: RefCell<String>,
    pub(crate) active: Cell<bool>,
}

// The central trait for subclassing a GObject
#[glib::object_subclass]
impl ObjectSubclass for WorkspaceObject {
    const NAME: &'static str = "WorkspaceObject";
    type Type = super::WorkspaceObject;
    type ParentType = glib::Object;
}

// Trait shared by all GObjects
impl ObjectImpl for WorkspaceObject {
    fn properties() -> &'static [ParamSpec] {
        static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
            vec![
                ParamSpecString::new(
                    // Name
                    "id",
                    // Nickname
                    "id",
                    // Short description
                    "id",
                    // Default value
                    None,
                    // The property can be read and written to
                    ParamFlags::READWRITE,
                ),
                ParamSpecBoolean::new(
                    "active",
                    "active",
                    "Indicates whether workspace is active",
                    false,
                    ParamFlags::READWRITE,
                ),
            ]
        });
        PROPERTIES.as_ref()
    }

    fn set_property(&self, _obj: &Self::Type, _id: usize, value: &Value, pspec: &ParamSpec) {
        match pspec.name() {
            "active" => {
                self.active
                    .replace(value.get().expect("Value needs to be a boolean"));
            }
            "id" => {
                self.id
                    .replace(value.get().expect("Value needs to be a boolean"));
            }
            _ => unimplemented!(),
        }
    }

    fn property(&self, _obj: &Self::Type, _id: usize, pspec: &ParamSpec) -> Value {
        match pspec.name() {
            "id" => self.id.borrow().to_value(),
            "active" => self.active.get().to_value(),
            _ => unimplemented!(),
        }
    }
}
