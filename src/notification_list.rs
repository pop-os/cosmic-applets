use cascade::cascade;
use gtk4::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use std::{cell::RefCell, collections::HashMap};

use crate::deref_cell::DerefCell;
use crate::notification_widget::NotificationWidget;
use crate::notifications::{Notification, NotificationId, Notifications};

#[derive(Default)]
pub struct NotificationListInner {
    listbox: DerefCell<gtk4::ListBox>,
    notifications: DerefCell<Notifications>,
    rows: RefCell<HashMap<NotificationId, gtk4::ListBoxRow>>,
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
            ..connect_row_activated(clone!(@weak obj => move |_, row| {
                if let Some(id) = obj.id_for_row(row) {
                    // TODO
                }
            }));
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

        let row = cascade! {
            gtk4::ListBoxRow::new();
            ..set_selectable(false);
            ..set_child(Some(&notification_widget));
        };

        self.inner().listbox.prepend(&row);
        self.inner().rows.borrow_mut().insert(notification.id, row);
    }

    fn id_for_row(&self, row: &gtk4::ListBoxRow) -> Option<NotificationId> {
        let rows = self.inner().rows.borrow();
        Some(*rows.iter().find(|(_, i)| i == &row)?.0)
    }
}
