use cascade::cascade;
use gtk4::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::deref_cell::DerefCell;

#[derive(Default)]
pub struct NotificationPopoverInner {
    label: DerefCell<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationPopoverInner {
    const NAME: &'static str = "S76NotificationPopover";
    type ParentType = gtk4::Popover;
    type Type = NotificationPopover;
}

impl ObjectImpl for NotificationPopoverInner {
    fn constructed(&self, obj: &NotificationPopover) {
        obj.add_controller(&cascade! {
            gtk4::GestureClick::new();
            ..connect_pressed(clone!(@weak obj => move |_, _, _, _| {
                obj.popdown();
            }));
        });

        let label = cascade! {
            gtk4::Label::new(None);
        };

        cascade! {
            obj;
            ..set_autohide(false);
            ..set_has_arrow(false);
            ..set_offset(0, 12);
            ..set_child(Some(&label));
        };

        self.label.set(label);
    }
}

impl WidgetImpl for NotificationPopoverInner {}
impl PopoverImpl for NotificationPopoverInner {}

glib::wrapper! {
    pub struct NotificationPopover(ObjectSubclass<NotificationPopoverInner>)
        @extends gtk4::Popover, gtk4::Widget;
}

impl NotificationPopover {
    pub fn new() -> Self {
        let obj = glib::Object::new::<Self>(&[]).unwrap();
        obj
    }

    fn inner(&self) -> &NotificationPopoverInner {
        NotificationPopoverInner::from_instance(self)
    }

    pub fn set_body(&self, body: &str) {
        self.inner().label.set_label(body);
    }
}
