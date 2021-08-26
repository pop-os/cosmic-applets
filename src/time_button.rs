use cascade::cascade;
use gtk4::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::deref_cell::DerefCell;
use crate::mpris::MprisControls;

#[derive(Default)]
pub struct TimeButtonInner {
    menu_button: DerefCell<gtk4::MenuButton>,
}

#[glib::object_subclass]
impl ObjectSubclass for TimeButtonInner {
    const NAME: &'static str = "S76TimeButton";
    type ParentType = gtk4::Widget;
    type Type = TimeButton;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for TimeButtonInner {
    fn constructed(&self, obj: &TimeButton) {
        let menu_button = cascade! {
            gtk4::MenuButton::new();
            ..set_parent(obj);
            ..set_direction(gtk4::ArrowType::None);
            ..set_popover(Some(&cascade! {
                gtk4::Popover::new();
                ..set_child(Some(&cascade! {
                    gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                    ..append(&MprisControls::new());
                    ..append(&cascade! {
                        gtk4::Calendar::new();
                    });
                }));
            }));
        };

        self.menu_button.set(menu_button);

        // TODO: better way to do this?
        glib::timeout_add_seconds_local(
            1,
            clone!(@weak obj => @default-return glib::Continue(false), move || {
                obj.update_time();
                glib::Continue(true)
            }),
        );
        obj.update_time();
    }

    fn dispose(&self, obj: &TimeButton) {
        self.menu_button.unparent();
    }
}

impl WidgetImpl for TimeButtonInner {}

glib::wrapper! {
    pub struct TimeButton(ObjectSubclass<TimeButtonInner>)
        @extends gtk4::Widget;
}

impl TimeButton {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }

    fn inner(&self) -> &TimeButtonInner {
        TimeButtonInner::from_instance(self)
    }

    fn update_time(&self) {
        // TODO: Locale-based formatting?
        let time = chrono::Local::now();
        self.inner()
            .menu_button
            .set_label(&time.format("%b %-d %-I:%M %p").to_string());
        // time.format("%B %-d %Y")
    }
}
