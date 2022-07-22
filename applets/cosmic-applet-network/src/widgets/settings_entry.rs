// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::{
    glib::{self, Object},
    prelude::*,
    subclass::prelude::*,
    Label,
};
use std::cell::RefCell;

glib::wrapper! {
    pub struct SettingsEntry(ObjectSubclass<SettingsEntryImp>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible;
}

impl SettingsEntry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_child<'a, Widget, IntoWidget>(&self, child: IntoWidget)
    where
        Widget: IsA<gtk4::Widget>,
        IntoWidget: Into<Option<&'a Widget>>,
    {
        let imp = self.inner();
        let child = child.into().map(AsRef::as_ref);
        let child_box_ref = imp.child_box.borrow();
        let child_box: &gtk4::Box = child_box_ref.as_ref().expect("child_box not created??");
        if let Some(new_child) = child {
            new_child.set_halign(gtk4::Align::End);
            child_box.append(new_child);
        }
        if let Some(old_child) = imp.child.replace(child.cloned()) {
            child_box.remove(&old_child);
        }
    }

    pub fn set_child_label<A: AsRef<str>>(&self, label: A) {
        let label = label.as_ref();
        let child = Label::builder()
            .label(label)
            .css_classes(vec!["settings-entry-text".into()])
            .build();
        self.set_child(&child);
    }

    pub fn align_child(&self, alignment: gtk4::Align) {
        let imp = self.inner();
        let child_box = imp.child_box.borrow();
        let child_box = child_box.as_ref().expect("child_box not created??");
        let child = imp.child.borrow();
        let child = child.as_ref().expect("child not set");
        let title_desc_box = imp.title_desc_box.borrow();
        let title_desc_box = title_desc_box
            .as_ref()
            .expect("title_desc_box not created?");
        match alignment {
            gtk4::Align::Start => {
                child_box.reorder_child_after(title_desc_box, Some(child));
            }
            gtk4::Align::End => {
                child_box.reorder_child_after(child, Some(title_desc_box));
            }
            _ => unimplemented!(),
        }
    }

    pub fn set_title(&self, title: &str) {
        let inner = self.inner();
        let title_ref = inner.title.borrow_mut();
        match &*title_ref {
            Some(label) => label.set_label(title),
            None => {
                let title = gtk4::Label::builder()
                    .label(title)
                    .css_classes(vec!["settings-entry-title".into()])
                    .halign(gtk4::Align::Start)
                    .build();
                let title_desc_box = inner.title_desc_box.borrow();
                let title_desc_box = title_desc_box
                    .as_ref()
                    .expect("title_desc_box not created?");
                if inner.desc.borrow().is_some() {
                    title_desc_box.prepend(&title);
                } else {
                    title_desc_box.append(&title);
                }
            }
        }
    }

    pub fn set_title_markup(&self, title: &str) {
        let inner = self.inner();
        let title_ref = inner.title.borrow_mut();
        match &*title_ref {
            Some(label) => label.set_markup(title),
            None => {
                let title = gtk4::Label::builder()
                    .label(title)
                    .use_markup(true)
                    .css_classes(vec!["settings-entry-title".into()])
                    .halign(gtk4::Align::Start)
                    .build();
                let title_desc_box = inner.title_desc_box.borrow();
                let title_desc_box = title_desc_box
                    .as_ref()
                    .expect("title_desc_box not created?");
                if inner.desc.borrow().is_some() {
                    title_desc_box.prepend(&title);
                } else {
                    title_desc_box.append(&title);
                }
            }
        }
    }

    pub fn set_description(&self, description: &str) {
        let inner = self.inner();
        let desc_ref = inner.desc.borrow_mut();
        match &*desc_ref {
            Some(label) => label.set_label(description),
            None => {
                let desc = gtk4::Label::builder()
                    .label(description)
                    .css_classes(vec!["settings-entry-desc".into()])
                    .halign(gtk4::Align::Start)
                    .build();
                let title_desc_box = inner.title_desc_box.borrow();
                let title_desc_box = title_desc_box
                    .as_ref()
                    .expect("title_desc_box not created?");
                title_desc_box.append(&desc);
            }
        }
    }

    fn inner(&self) -> &SettingsEntryImp {
        SettingsEntryImp::from_instance(self)
    }
}

impl Default for SettingsEntry {
    fn default() -> Self {
        Object::new(&[]).expect("Failed to create `SettingsEntry`.")
    }
}

#[derive(Debug, Default)]
pub struct SettingsEntryImp {
    title: RefCell<Option<gtk4::Label>>,
    desc: RefCell<Option<gtk4::Label>>,
    title_desc_box: RefCell<Option<gtk4::Box>>,
    child_box: RefCell<Option<gtk4::Box>>,
    child: RefCell<Option<gtk4::Widget>>,
}

#[glib::object_subclass]
impl ObjectSubclass for SettingsEntryImp {
    const NAME: &'static str = "SettingsEntry";
    type Type = SettingsEntry;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        // The layout manager determines how child widgets are laid out.
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for SettingsEntryImp {
    fn constructed(&self, obj: &Self::Type) {
        self.parent_constructed(obj);
        let child = gtk4::Box::builder()
            .css_classes(vec!["settings-entry".into()])
            .orientation(gtk4::Orientation::Horizontal)
            .hexpand(true)
            .margin_start(24)
            .margin_end(24)
            .margin_top(8)
            .margin_bottom(8)
            .spacing(16)
            .build();

        let title_and_desc = gtk4::Box::builder()
            .css_classes(vec!["settings-entry-info".into()])
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .build();
        child.append(&title_and_desc);
        *self.title_desc_box.borrow_mut() = Some(title_and_desc);
        if let Some(entry_child) = self.child.borrow().as_ref() {
            child.append(entry_child);
        }
        child.set_parent(obj);
        *self.child_box.borrow_mut() = Some(child);
    }

    fn dispose(&self, _obj: &Self::Type) {
        if let Some(child) = self.child.borrow_mut().take() {
            child.unparent();
        }
        if let Some(child_box) = self.child_box.borrow_mut().take() {
            child_box.unparent();
        }
    }
}

impl WidgetImpl for SettingsEntryImp {}
