use crate::Activate;
use gtk4::{glib, subclass::prelude::*, ToggleButton};
use once_cell::sync::OnceCell;
use std::{cell::RefCell, rc::Rc};
use tokio::sync::mpsc;

// Object holding the state
#[derive(Default)]
pub struct WorkspaceButton {
    pub button: Rc<RefCell<ToggleButton>>,
}

// The central trait for subclassing a GObject
#[glib::object_subclass]
impl ObjectSubclass for WorkspaceButton {
    const NAME: &'static str = "WorkspaceButton";
    type Type = super::WorkspaceButton;
    type ParentType = gtk4::Box;
}

// Trait shared by all GObjects
impl ObjectImpl for WorkspaceButton {}

// Trait shared by all widgets
impl WidgetImpl for WorkspaceButton {}

// Trait shared by all buttons
impl BoxImpl for WorkspaceButton {}
