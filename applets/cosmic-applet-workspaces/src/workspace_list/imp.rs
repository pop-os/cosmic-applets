// SPDX-License-Identifier: MPL-2.0-only

use cosmic_panel_config::config::CosmicPanelConfig;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib, EventControllerScroll};
use gtk4::{Box, ListView};
use once_cell::sync::OnceCell;
use tokio::sync::mpsc;

use crate::utils::Activate;

#[derive(Debug, Default)]
pub struct WorkspaceList {
    pub list_view: OnceCell<ListView>,
    pub model: OnceCell<gio::ListStore>,
    pub config: OnceCell<CosmicPanelConfig>,
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
