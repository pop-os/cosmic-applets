use cascade::cascade;
use gtk4::{
    glib::{self, clone},
    pango,
    prelude::*,
    subclass::prelude::*,
};

use crate::deref_cell::DerefCell;
use crate::notifications::Notification;

#[derive(Default)]
pub struct NotificationPopoverInner {
    summary_label: DerefCell<gtk4::Label>,
    body_label: DerefCell<gtk4::Label>,
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

        let summary_label = cascade! {
            gtk4::Label::new(None);
            ..set_attributes(Some(&cascade! {
                pango::AttrList::new();
                ..insert(pango::Attribute::new_weight(pango::Weight::Bold));
            }));
        };

        let body_label = cascade! {
            gtk4::Label::new(None);
        };

        cascade! {
            obj;
            ..set_autohide(false);
            ..set_has_arrow(false);
            ..set_offset(0, 12);
            ..set_child(Some(&cascade! {
                gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                ..append(&summary_label);
                ..append(&body_label);
            }));
        };

        self.summary_label.set(summary_label);
        self.body_label.set(body_label);
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

    pub fn set_notification(&self, notification: &Notification) {
        self.inner().summary_label.set_label(&notification.summary);
        self.inner().body_label.set_label(&notification.body);
    }
}
