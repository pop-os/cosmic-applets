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
pub struct DockList {
    pub list_view: OnceCell<ListView>,
    pub type_: OnceCell<super::DockListType>,
    pub model: OnceCell<gio::ListStore>,
    pub click_controller: OnceCell<GestureClick>,
    pub drop_controller: OnceCell<DropTarget>,
    pub drag_source: OnceCell<DragSource>,
    pub drag_end_signal: Rc<RefCell<Option<SignalHandlerId>>>,
    pub drag_cancel_signal: Rc<RefCell<Option<SignalHandlerId>>>,
    pub popover_menu_index: Rc<Cell<Option<u32>>>,
    pub position: Rc<Cell<Anchor>>,
    pub tx: OnceCell<mpsc::Sender<Event>>,
    pub config: OnceCell<CosmicPanelConfig>
}

#[glib::object_subclass]
impl ObjectSubclass for DockList {
    const NAME: &'static str = "DockList";
    type Type = super::DockList;
    type ParentType = Box;
}

impl ObjectImpl for DockList {}

impl WidgetImpl for DockList {}

impl BoxImpl for DockList {}
