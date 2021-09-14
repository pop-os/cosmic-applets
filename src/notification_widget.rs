use cascade::cascade;
use gtk4::{
    glib::{self, clone},
    pango,
    prelude::*,
    subclass::prelude::*,
};
use std::cell::Cell;

use crate::deref_cell::DerefCell;
use crate::notifications::{Notification, NotificationId, Notifications};

#[derive(Default)]
pub struct NotificationWidgetInner {
    box_: DerefCell<gtk4::Box>,
    summary_label: DerefCell<gtk4::Label>,
    body_label: DerefCell<gtk4::Label>,
    notifications: DerefCell<Notifications>,
    id: Cell<Option<NotificationId>>,
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
            gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            ..set_parent(obj);
            ..append(&cascade! {
                gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                ..append(&summary_label);
                ..append(&body_label);
            });
                ..append(&cascade! {
                    gtk4::Button::new();
                    ..style_context().add_provider(&cascade! {
                        gtk4::CssProvider::new();
                        ..load_from_data(b"button { min-width: 0; min-height: 0; padding: 4px 4px; }");
                    }, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
                    ..style_context().add_class("flat");
                    ..set_valign(gtk4::Align::Start);
                    ..set_child(Some(&cascade! {
                        gtk4::Image::from_icon_name(Some("window-close-symbolic"));
                        ..set_pixel_size(8);
                    }));
                    ..connect_clicked(clone!(@weak obj => move |_| {
                        if let Some(id) = obj.id() {
                            obj.inner().notifications.dismiss(id);
                        }
                    }));
                });

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
    pub fn new(notifications: &Notifications) -> Self {
        let obj = glib::Object::new::<Self>(&[]).unwrap();
        obj.inner().notifications.set(notifications.clone());
        obj
    }

    fn inner(&self) -> &NotificationWidgetInner {
        NotificationWidgetInner::from_instance(self)
    }

    pub fn set_notification(&self, notification: &Notification) {
        self.inner().summary_label.set_label(&notification.summary);
        self.inner().body_label.set_label(&notification.body);
        self.inner().id.set(Some(notification.id));
    }

    pub fn id(&self) -> Option<NotificationId> {
        self.inner().id.get()
    }
}
