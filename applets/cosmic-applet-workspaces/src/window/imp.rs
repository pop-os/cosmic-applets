// SPDX-License-Identifier: MPL-2.0-only

use crate::workspace_list::WorkspaceList;
use gtk4::{glib, subclass::prelude::*};
use once_cell::sync::OnceCell;

// Object holding the state
#[derive(Default)]
pub struct CosmicWorkspacesWindow {
    pub(super) inner: OnceCell<WorkspaceList>,
}

// The central trait for subclassing a GObject
#[glib::object_subclass]
impl ObjectSubclass for CosmicWorkspacesWindow {
    // `NAME` needs to match `class` attribute of template
    const NAME: &'static str = "CosmicWorkspacesWindow";
    type Type = super::CosmicWorkspacesWindow;
    type ParentType = gtk4::ApplicationWindow;
}

// Trait shared by all GObjects
impl ObjectImpl for CosmicWorkspacesWindow {}

// Trait shared by all widgets
impl WidgetImpl for CosmicWorkspacesWindow {}

// Trait shared by all windows
impl WindowImpl for CosmicWorkspacesWindow {}

// Trait shared by all application
impl ApplicationWindowImpl for CosmicWorkspacesWindow {}
