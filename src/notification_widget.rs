use cascade::cascade;
use gtk4::{glib, pango, prelude::*, subclass::prelude::*};

use crate::deref_cell::DerefCell;
use crate::notifications::Notification;

#[derive(Default)]
pub struct NotificationWidgetInner {
    box_: DerefCell<gtk4::Box>,
    summary_label: DerefCell<gtk4::Label>,
    body_label: DerefCell<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationWidgetInner {
    const NAME: &'static str = "S76NotificationWidget";
    type ParentType = gtk4::Widget;
    type Type = NotificationWidget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for NotificationWidgetInner {
    fn constructed(&self, obj: &NotificationWidget) {
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

        let box_ = cascade! {
            gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            ..set_parent(obj);
            ..append(&summary_label);
            ..append(&body_label);
        };

        self.box_.set(box_);
        self.summary_label.set(summary_label);
        self.body_label.set(body_label);
    }

    fn dispose(&self, _obj: &NotificationWidget) {
        self.box_.unparent();
    }
}

impl WidgetImpl for NotificationWidgetInner {}

glib::wrapper! {
    pub struct NotificationWidget(ObjectSubclass<NotificationWidgetInner>)
        @extends gtk4::Widget;
}

impl NotificationWidget {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }

    fn inner(&self) -> &NotificationWidgetInner {
        NotificationWidgetInner::from_instance(self)
    }

    pub fn set_notification(&self, notification: &Notification) {
        self.inner().summary_label.set_label(&notification.summary);
        self.inner().body_label.set_label(&notification.body);
    }
}
