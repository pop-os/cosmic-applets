// SPDX-License-Identifier: MPL-2.0-only

use crate::dock_object::DockObject;
use crate::dock_popover::DockPopover;
use crate::utils::BoxedWindowList;
use crate::utils::Event;
use cascade::cascade;
use cosmic_panel_config::config::Anchor;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::Box;
use gtk4::Image;
use gtk4::Orientation;
use gtk4::Popover;
use gtk4::{Align, PositionType};
use tokio::sync::mpsc::Sender;

mod imp;

glib::wrapper! {
    pub struct DockItem(ObjectSubclass<imp::DockItem>)
        @extends gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl DockItem {
    pub fn new(tx: Sender<Event>, icon_size: u32) -> Self {
        let self_: DockItem = glib::Object::new(&[]).expect("Failed to create DockItem");

        let item_box = Box::new(Orientation::Vertical, 0);
        item_box.add_css_class("transparent");
        cascade! {
            &self_;
            ..set_child(Some(&item_box));
            ..add_css_class("dock_item");
        };

        let image = cascade! {
            Image::new();
            ..set_hexpand(true);
            ..set_halign(Align::Center);
            ..set_pixel_size(icon_size.try_into().unwrap());
            ..add_css_class("dock");
        };
        let dots = cascade! {
            Box::new(Orientation::Horizontal, 4);
            ..set_hexpand(true);
            ..set_halign(Align::Center);
            ..set_valign(Align::Center);
            ..add_css_class("transparent");
        };
        // TODO dots inverse color of parent with gsk blend modes?
        item_box.append(&image);
        item_box.append(&dots);
        let popover = cascade! {
            Popover::new();
            ..set_autohide(true);
            ..add_css_class("dock");
            ..set_has_arrow(false);
        };
        item_box.append(&popover);
        let self_clone = self_.clone();
        popover.connect_closed(move |_| {
            let _ = self_clone.emit_by_name::<()>("popover-closed", &[]);
        });

        let popover_menu = cascade! {
            DockPopover::new(tx.clone());
            ..add_css_class("popover_menu");
        };
        popover.set_child(Some(&popover_menu));
        popover_menu.connect_local(
            "menu-hide",
            false,
            glib::clone!(@weak popover, @weak popover_menu => @default-return None, move |_| {
                popover.popdown();
                popover_menu.reset_menu();
                None
            }),
        );

        let imp = imp::DockItem::from_instance(&self_);
        imp.icon_size.set(icon_size);
        imp.image.replace(Some(image));
        imp.dots.replace(dots);
        imp.item_box.replace(item_box);
        imp.popover.replace(popover);
        imp.popover_menu.replace(Some(popover_menu));
        imp.tx.set(tx).unwrap();
        self_
    }

    // refactor to emit event for removing the item?
    pub fn set_dock_object(&self, dock_object: &DockObject) {
        let imp = imp::DockItem::from_instance(self);
        let image = cascade! {
            dock_object.get_image();
            ..set_hexpand(true);
            ..set_halign(Align::Center);
            ..set_pixel_size(imp.icon_size.get().try_into().unwrap());
            ..set_tooltip_text(dock_object.get_name().as_deref());
        };
        let old_image = imp.image.replace(None);
        if let Some(old_image) = old_image {
            imp.item_box.borrow().remove(&old_image);
            imp.item_box.borrow().prepend(&image);
            imp.image.replace(Some(image));
        }
        let active = dock_object.property::<BoxedWindowList>("active");
        let dots = imp.dots.borrow();
        while let Some(c) = dots.first_child() {
            dots.remove(&c);
        }
        for _ in active.0 {
            dots.append(&cascade! {
                Box::new(Orientation::Horizontal, 0);
                ..set_halign(Align::Center);
                ..set_valign(Align::Center);
                ..add_css_class("dock_dots");
            });
        }

        let popover = dock_object.property::<bool>("popover");
        // dbg!(popover);
        // dbg!(dock_object);
        if popover {
            self.add_popover(dock_object);
        } else {
            self.clear_popover();
        }
    }

    pub fn set_position(&self, position: Anchor) {
        let imp = imp::DockItem::from_instance(self);
        let item_box = imp.item_box.borrow();
        let dots = imp.dots.borrow();
        if let Some(image) = imp.image.borrow().as_ref() {
            match position {
                Anchor::Left => {
                    item_box.set_orientation(Orientation::Horizontal);
                    dots.set_orientation(Orientation::Vertical);
                    dots.set_margin_bottom(4);
                    dots.set_margin_top(4);
                    item_box.reorder_child_after(&image.clone(), Some(&dots.clone()));
                }
                Anchor::Right => {
                    item_box.set_orientation(Orientation::Horizontal);
                    dots.set_orientation(Orientation::Vertical);
                    dots.set_margin_bottom(4);
                    dots.set_margin_top(4);
                    item_box.reorder_child_after(&dots.clone(), Some(&image.clone()));
                }
                Anchor::Top => {
                    item_box.set_orientation(Orientation::Vertical);
                    dots.set_orientation(Orientation::Horizontal);
                    dots.set_margin_start(4);
                    dots.set_margin_end(4);
                    item_box.reorder_child_after(&image.clone(), Some(&dots.clone()));
                }
                Anchor::Bottom => {
                    item_box.set_orientation(Orientation::Vertical);
                    dots.set_orientation(Orientation::Horizontal);
                    dots.set_margin_start(4);
                    dots.set_margin_end(4);
                    item_box.reorder_child_after(&dots.clone(), Some(&image.clone()));
                }
            };
        }
        let popover = imp.popover.borrow();
        popover.set_position(match position {
            Anchor::Left => PositionType::Right,
            Anchor::Right => PositionType::Left,
            Anchor::Top => PositionType::Bottom,
            Anchor::Bottom => PositionType::Top,
        });
        
    }

    pub fn add_popover(&self, obj: &DockObject) {
        let imp = imp::DockItem::from_instance(self);
        let popover = imp.popover.borrow();
        if let Some(popover_menu) = imp.popover_menu.borrow().as_ref() {
            popover_menu.set_dock_object(obj, true);
            popover.popup();
        }
    }

    pub fn clear_popover(&self) {
        let imp = imp::DockItem::from_instance(self);
        let popover = imp.popover.borrow();
        if let Some(popover_menu) = imp.popover_menu.borrow().as_ref() {
            popover.popdown();
            popover_menu.reset_menu();
        }
    }
}
