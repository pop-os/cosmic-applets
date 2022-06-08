// SPDX-License-Identifier: MPL-2.0-only

use cosmic_panel_config::config::{CosmicPanelConfig};
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use gtk4::{Box, ListView};
use tokio::sync::mpsc;
use once_cell::sync::OnceCell;

use crate::utils::Event;

#[derive(Debug, Default)]
pub struct WorkspaceList {
    pub list_view: OnceCell<ListView>,
    pub model: OnceCell<gio::ListStore>,
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
