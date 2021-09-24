use cascade::cascade;
use gtk4::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use std::cell::RefCell;

use crate::deref_cell::DerefCell;
use crate::notification_widget::NotificationWidget;
use crate::notifications::{Notification, NotificationId, Notifications};

#[derive(Default)]
pub struct NotificationPopoverInner {
    notification_widget: DerefCell<NotificationWidget>,
    notifications: DerefCell<Notifications>,
    source: RefCell<Option<glib::SourceId>>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationPopoverInner {
    const NAME: &'static str = "S76NotificationPopover";
    type ParentType = gtk4::Popover;
    type Type = NotificationPopover;
}

impl ObjectImpl for NotificationPopoverInner {
    fn constructed(&self, obj: &NotificationPopover) {
        cascade! {
            obj;
            ..set_autohide(false);
            ..set_has_arrow(false);
            ..set_offset(0, 12);
            ..add_controller(&cascade! {
                gtk4::GestureClick::new();
                ..connect_released(clone!(@weak obj => move |_, n_press, _, _| {
                    if n_press != 1 {
                        return;
                    }
                    if let Some(id) = obj.id() {
                        obj.inner().notifications.invoke_action(id, "default");
                    }
                    obj.popdown();
                    obj.stop_timer();
                }));
            });
            ..add_controller(&cascade! {
                gtk4::EventControllerMotion::new();
                ..connect_enter(clone!(@weak obj => move |_, _, _| {
                    obj.stop_timer();
                }));
                ..connect_leave(clone!(@weak obj => move |_| {
                    obj.start_timer();
                }));
            });
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
            if obj.id() == Some(id) {
                obj.popdown();
                obj.stop_timer();
            }
        }));

        obj
    }

    fn inner(&self) -> &NotificationPopoverInner {
        NotificationPopoverInner::from_instance(self)
    }

    fn id(&self) -> Option<NotificationId> {
        self.inner().notification_widget.id()
    }

    fn handle_notification(&self, notification: &Notification) {
        self.inner()
            .notification_widget
            .set_notification(notification);
        self.popup();
        self.start_timer();
    }

    fn stop_timer(&self) {
        if let Some(source) = self.inner().source.borrow_mut().take() {
            glib::source_remove(source);
        }
    }

    fn start_timer(&self) {
        self.stop_timer();
        let source = glib::timeout_add_seconds_local(
            1,
            clone!(@weak self as self_ => @default-return Continue(false), move || {
                self_.popdown();
                Continue(false)
            }),
        );
        *self.inner().source.borrow_mut() = Some(source);
    }
}
