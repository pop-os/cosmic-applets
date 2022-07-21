// SPDX-License-Identifier: MPL-2.0-only

use gtk4::{
    glib::{self, subclass::Signal},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::dock_popover::DockPopover;

#[derive(Debug, Default)]
pub struct DockItem {
    pub image: Rc<RefCell<Option<gtk4::Image>>>,
    pub dots: Rc<RefCell<gtk4::Box>>,
    pub item_box: Rc<RefCell<gtk4::Box>>,
    pub popover: Rc<RefCell<gtk4::Popover>>,
    pub popover_menu: Rc<RefCell<Option<DockPopover>>>,
    pub icon_size: Rc<Cell<u32>>,
}

#[glib::object_subclass]
impl ObjectSubclass for DockItem {
    const NAME: &'static str = "DockItem";
    type Type = super::DockItem;
    type ParentType = gtk4::Button;
}

impl ObjectImpl for DockItem {
    fn signals() -> &'static [Signal] {
        static SIGNALS: Lazy<Vec<Signal>> =
            Lazy::new(|| vec![Signal::builder("popover-closed").build()]);
        SIGNALS.as_ref()
    }
}

impl WidgetImpl for DockItem {}

impl ButtonImpl for DockItem {}
