// SPDX-License-Identifier: MPL-2.0-only

use gtk4::{glib, subclass::prelude::*};
// Object holding the state
#[derive(Default)]

pub struct CosmicPanelAppButtonWindow {}

// The central trait for subclassing a GObject
#[glib::object_subclass]
impl ObjectSubclass for CosmicPanelAppButtonWindow {
    // `NAME` needs to match `class` attribute of template
    const NAME: &'static str = "CosmicPanelAppButtonWindow";
    type Type = super::CosmicPanelAppButtonWindow;
    type ParentType = gtk4::ApplicationWindow;
}

// Trait shared by all GObjects
impl ObjectImpl for CosmicPanelAppButtonWindow {}

// Trait shared by all widgets
impl WidgetImpl for CosmicPanelAppButtonWindow {}

// Trait shared by all windows
impl WindowImpl for CosmicPanelAppButtonWindow {}

// Trait shared by all application
impl ApplicationWindowImpl for CosmicPanelAppButtonWindow {}
