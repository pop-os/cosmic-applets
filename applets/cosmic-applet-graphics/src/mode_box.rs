// SPDX-License-Identifier: LGPL-3.0-or-later

use gtk4::{
    glib::{self, Object},
    prelude::*,
    subclass::prelude::*,
    Align, CheckButton, Label, Orientation,
};

glib::wrapper! {
    pub struct ModeSelection(ObjectSubclass<ModeSelectionImp>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible;
}

impl ModeSelection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_title(&self, title: &str) {
        self.inner().label.set_text(title);
    }

    pub fn set_description(&self, title: &str) {
        let inner = self.inner();
        inner.description.set_text(title);
        inner.description.show();
    }

    pub fn is_active(&self) -> bool {
        self.inner().check.is_active()
    }

    pub fn set_active(&self, setting: bool) {
        self.inner().check.set_active(setting)
    }

    pub fn set_group(&self, group: Option<&impl IsA<CheckButton>>) {
        self.inner().check.set_group(group)
    }

    pub fn connect_toggled<F: Fn(&CheckButton) + 'static>(&self, f: F) {
        self.inner().check.connect_toggled(f);
    }

    fn inner(&self) -> &ModeSelectionImp {
        ModeSelectionImp::from_instance(self)
    }
}

impl Default for ModeSelection {
    fn default() -> Self {
        Object::new(&[]).expect("Failed to create `ModeSelection`.")
    }
}

#[derive(Debug, Default)]
pub struct ModeSelectionImp {
    inner_box: gtk4::Box,
    label_box: gtk4::Box,
    label: Label,
    description: Label,
    check: CheckButton,
}

#[glib::object_subclass]
impl ObjectSubclass for ModeSelectionImp {
    const NAME: &'static str = "ModeSelection";
    type Type = ModeSelection;
    type ParentType = gtk4::Widget;
    type Interfaces = ();
    type Instance = glib::subclass::basic::InstanceStruct<Self>;
    type Class = glib::subclass::basic::ClassStruct<Self>;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for ModeSelectionImp {
    fn constructed(&self, obj: &Self::Type) {
        self.parent_constructed(obj);

        self.check.set_halign(Align::End);

        self.label.set_halign(Align::Start);
        self.label.add_css_class("title");

        self.description.set_halign(Align::Start);
        self.description.add_css_class("description");
        self.description.hide();

        self.label_box.set_orientation(Orientation::Vertical);
        self.label_box.append(&self.label);
        self.label_box.append(&self.description);

        self.inner_box.set_orientation(Orientation::Horizontal);
        self.inner_box.append(&self.label_box);
        self.inner_box.append(&self.check);
    }

    fn dispose(&self, _obj: &Self::Type) {
        self.inner_box.remove(&self.label);
        self.inner_box.remove(&self.check);
        self.inner_box.unparent();
    }
}

impl WidgetImpl for ModeSelectionImp {}
