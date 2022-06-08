use std::{rc::Rc, cell::RefCell};
use gtk4::{ToggleButton, glib, subclass::prelude::*};
use tokio::sync::mpsc;
use once_cell::sync::OnceCell;
use crate::Event;

// Object holding the state
#[derive(Default)]
pub struct WorkspaceButton {
    pub tx: Rc<OnceCell<mpsc::Sender<Event>>>,
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