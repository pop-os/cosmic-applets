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
pub struct NotificationListInner {
    listbox: DerefCell<gtk4::ListBox>,
    notifications: DerefCell<Notifications>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationListInner {
    const NAME: &'static str = "S76NotificationList";
    type ParentType = gtk4::Widget;
    type Type = NotificationList;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for NotificationListInner {
    fn constructed(&self, obj: &NotificationList) {
        let listbox = cascade! {
            gtk4::ListBox::new();
            ..set_parent(obj);
        };

        self.listbox.set(listbox);
    }

    fn dispose(&self, _obj: &NotificationList) {
        self.listbox.unparent();
    }
}

impl WidgetImpl for NotificationListInner {}

glib::wrapper! {
    pub struct NotificationList(ObjectSubclass<NotificationListInner>)
        @extends gtk4::Widget;
}

impl NotificationList {
    pub fn new(notifications: &Notifications) -> Self {
        let obj = glib::Object::new::<Self>(&[]).unwrap();

        // XXX disconnect?
        obj.inner().notifications.set(notifications.clone());
        notifications.connect_notification_recieved(clone!(@weak obj => move |notification| {
            obj.handle_notification(&notification);
        }));

        obj
    }

    fn inner(&self) -> &NotificationListInner {
        NotificationListInner::from_instance(self)
    }

    fn handle_notification(&self, notification: &Notification) {
        let notification_widget = cascade! {
            NotificationWidget::new();
            ..set_notification(notification);
        };

        self.inner().listbox.prepend(&notification_widget);
    }
}
