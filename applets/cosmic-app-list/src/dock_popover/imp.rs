// SPDX-License-Identifier: MPL-2.0-only

use std::cell::RefCell;
use std::rc::Rc;

use glib::subclass::Signal;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{Box, Button, ListBox, Revealer};
use once_cell::sync::Lazy;

use crate::dock_object::DockObject;

#[derive(Debug, Default)]
pub struct DockPopover {
    pub menu_handle: Rc<RefCell<Box>>,
    pub all_windows_item_revealer: Rc<RefCell<Revealer>>,
    pub all_windows_item_header: Rc<RefCell<Button>>,
    pub window_list: Rc<RefCell<ListBox>>,
    pub launch_new_item: Rc<RefCell<Button>>,
    pub favorite_item: Rc<RefCell<Button>>,
    pub quit_all_item: Rc<RefCell<Button>>,
    //TODO figure out how to use lifetimes with glib::wrapper! macro
    pub dock_object: Rc<RefCell<Option<DockObject>>>,
}

#[glib::object_subclass]
impl ObjectSubclass for DockPopover {
    const NAME: &'static str = "DockPopover";
    type Type = super::DockPopover;
    type ParentType = Box;
}

impl ObjectImpl for DockPopover {
    fn signals() -> &'static [Signal] {
        static SIGNALS: Lazy<Vec<Signal>> =
            Lazy::new(|| vec![Signal::builder("menu-hide").build()]);
        SIGNALS.as_ref()
    }
}

impl WidgetImpl for DockPopover {}

impl BoxImpl for DockPopover {}
