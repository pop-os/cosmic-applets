use cascade::cascade;
use gtk4::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::deref_cell::DerefCell;
use crate::notification_widget::NotificationWidget;
use crate::notifications::{Notification, Notifications};

#[derive(Default)]
pub struct NotificationPopoverInner {
    notification_widget: DerefCell<NotificationWidget>,
    notifications: DerefCell<Notifications>,
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

        cascade! {
            obj;
            ..set_autohide(false);
            ..set_has_arrow(false);
            ..set_offset(0, 12);
        };
    }
}

impl WidgetImpl for NotificationPopoverInner {}
impl PopoverImpl for NotificationPopoverInner {}

glib::wrapper! {
    pub struct NotificationPopover(ObjectSubclass<NotificationPopoverInner>)
        @extends gtk4::Popover, gtk4::Widget;
}

impl NotificationPopover {
    pub fn new(notifications: &Notifications) -> Self {
        let obj = glib::Object::new::<Self>(&[]).unwrap();

        let notification_widget = cascade! {
            NotificationWidget::new(notifications);
        };
        obj.set_child(Some(&notification_widget));
        obj.inner().notification_widget.set(notification_widget);

        // XXX disconnect?
        obj.inner().notifications.set(notifications.clone());
        notifications.connect_notification_recieved(clone!(@weak obj => move |notification| {
             obj.handle_notification(&notification);
        }));
        notifications.connect_notification_closed(clone!(@weak obj => move |id| {
            if obj.inner().notification_widget.id() == Some(id) {
                obj.popdown();
            }
        }));

        obj
    }

    fn inner(&self) -> &NotificationPopoverInner {
        NotificationPopoverInner::from_instance(self)
    }

    fn handle_notification(&self, notification: &Notification) {
        self.inner()
            .notification_widget
            .set_notification(notification);
        self.popup();
    }
}
