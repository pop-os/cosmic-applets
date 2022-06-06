// SPDX-License-Identifier: MPL-2.0-only

use glib::subclass::Signal;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio::sync::mpsc::Sender;

use crate::dock_popover::DockPopover;
use crate::utils::Event;

#[derive(Debug, Default)]
pub struct DockItem {
    pub image: Rc<RefCell<Option<gtk4::Image>>>,
    pub dots: Rc<RefCell<gtk4::Box>>,
    pub item_box: Rc<RefCell<gtk4::Box>>,
    pub popover: Rc<RefCell<gtk4::Popover>>,
    pub popover_menu: Rc<RefCell<Option<DockPopover>>>,
    pub tx: OnceCell<Sender<Event>>,
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
        static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
            vec![Signal::builder(
                // Signal name
                "popover-closed",
                // Types of the values which will be sent to the signal handler
                &[],
                // Type of the value the signal handler sends back
                <()>::static_type().into(),
            )
            .build()]
        });
        SIGNALS.as_ref()
    }
}

impl WidgetImpl for DockItem {}

impl ButtonImpl for DockItem {}
