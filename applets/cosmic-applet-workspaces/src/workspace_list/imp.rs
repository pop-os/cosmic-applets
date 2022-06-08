// SPDX-License-Identifier: MPL-2.0-only

use cosmic_panel_config::config::{Anchor, CosmicPanelConfig};
use glib::SignalHandlerId;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use gtk4::{Box, DragSource, DropTarget, GestureClick, ListView};
use once_cell::sync::OnceCell;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio::sync::mpsc;

use crate::utils::Event;

#[derive(Debug, Default)]
pub struct WorkspaceList {
    pub list_view: OnceCell<ListView>,
    pub model: OnceCell<gio::ListStore>,
    pub click_controller: OnceCell<GestureClick>,
    pub tx: OnceCell<mpsc::Sender<Event>>,
    pub config: OnceCell<CosmicPanelConfig>
}

#[glib::object_subclass]
impl ObjectSubclass for WorkspaceList {
    const NAME: &'static str = "WorkspaceList";
    type Type = super::WorkspaceList;
    type ParentType = Box;
}

impl ObjectImpl for WorkspaceList {}

impl WidgetImpl for WorkspaceList {}

impl BoxImpl for WorkspaceList {}
