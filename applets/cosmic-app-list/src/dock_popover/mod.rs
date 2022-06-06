// SPDX-License-Identifier: MPL-2.0-only

use cascade::cascade;
use gio::DesktopAppInfo;
use gtk4::gdk::pango::EllipsizeMode;
use gtk4::subclass::prelude::*;
use gtk4::{gdk, gio, glib};
use gtk4::{prelude::*, Label};
use gtk4::{Box, Button, Image, ListBox, Orientation};
use tokio::sync::mpsc::Sender;

use crate::dock_object::DockObject;
use crate::utils::BoxedWindowList;
use crate::utils::Event;

mod imp;

glib::wrapper! {
    pub struct DockPopover(ObjectSubclass<imp::DockPopover>)
        @extends gtk4::Widget, gtk4::Box,
    @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl DockPopover {
    pub fn new(tx: Sender<Event>) -> Self {
        let self_: DockPopover = glib::Object::new(&[]).expect("Failed to create DockList");
        let imp = imp::DockPopover::from_instance(&self_);
        imp.tx.set(tx).unwrap();
        self_.layout();
        //dnd behavior is different for each type, as well as the data in the model
        self_
    }

    pub fn set_dock_object(&self, dock_object: &DockObject, update_layout: bool) {
        let imp = imp::DockPopover::from_instance(self);
        imp.dock_object.replace(Some(dock_object.clone()));
        if update_layout {
            self.update_layout();
        }
    }

    pub fn update_layout(&self) {
        self.reset_menu();
        cascade! {
            &self;
            ..set_orientation(Orientation::Vertical);
            ..set_hexpand(true);
        };

        // build menu
        let imp = imp::DockPopover::from_instance(self);
        let dock_object = imp.dock_object.borrow();
        let menu_handle = imp.menu_handle.borrow();
        if let Some(dock_object) = dock_object.as_ref() {
            let all_windows_item_container = cascade! {
                Box::new(Orientation::Vertical, 4);
            };
            menu_handle.append(&all_windows_item_container);
            let window_list = dock_object.property::<BoxedWindowList>("active");
            if window_list.0.is_empty() {
                all_windows_item_container.hide();
            } else {
                let window_listbox = cascade! {
                    ListBox::new();
                    ..set_activate_on_single_click(true);
                    ..add_css_class("popover_menu");
                };
                all_windows_item_container.append(&window_listbox);
                for w in window_list.0 {
                    let window_box = cascade! {
                        Box::new(Orientation::Vertical, 4);
                        ..add_css_class("dock_item");
                    };
                    window_listbox.append(&window_box);

                    let window_title = cascade! {
                        Label::new(Some(w.name.as_str()));
                        ..set_margin_start(4);
                        ..set_margin_end(4);
                        ..set_margin_top(4);
                        ..set_margin_bottom(4);
                        ..set_wrap(true);
                        ..set_max_width_chars(20);
                        ..set_ellipsize(EllipsizeMode::End);
                        ..add_css_class("title-4");
                        ..add_css_class("dock_popover_title");
                    };

                    let window_image = cascade! {
                        //TODO fill with image of window
                        Image::from_pixbuf(None);
                    };
                    window_box.append(&window_image);
                    window_box.append(&window_title);
                }
                // imp.all_windows_item_revealer.replace(window_list_revealer);
                imp.window_list.replace(window_listbox);
            }

            let launch_item_container = cascade! {
                Box::new(Orientation::Vertical, 4);
                ..set_hexpand(true);
                ..add_css_class("popover_menu");
            };
            menu_handle.append(&launch_item_container);

            let launch_new_item = cascade! {
                Button::with_label("New Window");
                ..add_css_class("popover_menu");
            };
            launch_item_container.append(&launch_new_item);
            imp.launch_new_item.replace(launch_new_item);

            let favorite_item = cascade! {
                Button::with_label(if dock_object.property::<bool>("saved") {"Remove from Favorites"} else {"Add to Favorites"});
                ..add_css_class("popover_menu");
            };
            menu_handle.append(&favorite_item);
            imp.favorite_item.replace(favorite_item);

            let window_list = dock_object.property::<BoxedWindowList>("active");

            if window_list.0.len() > 1 {
                let quit_all_item = cascade! {
                    Button::with_label(format!("Quit {} Windows", window_list.0.len()).as_str());
                    ..add_css_class("popover_menu");
                };
                menu_handle.append(&quit_all_item);
                imp.quit_all_item.replace(quit_all_item);
            } else {
                let quit_all_item = cascade! {
                    Button::with_label("Quit");
                    ..add_css_class("popover_menu");
                };
                menu_handle.append(&quit_all_item);
                if window_list.0.is_empty() {
                    quit_all_item.hide();
                }
                imp.quit_all_item.replace(quit_all_item);
            }

            self.setup_handlers();
        }
    }

    fn layout(&self) {
        let imp = imp::DockPopover::from_instance(self);
        let menu_handle = cascade! {
            Box::new(Orientation::Vertical, 4);
            ..add_css_class("popover_menu");
        };
        self.append(&menu_handle);
        imp.menu_handle.replace(menu_handle);
    }

    fn emit_hide(&self) {
        self.emit_by_name::<()>("menu-hide", &[]);
    }

    pub fn reset_menu(&self) {
        // reset menu
        let menu_handle = cascade! {
            Box::new(Orientation::Vertical, 4);
        };
        self.append(&menu_handle);

        let imp = imp::DockPopover::from_instance(self);
        let old_menu_handle = imp.menu_handle.replace(menu_handle);
        self.remove(&old_menu_handle);
    }

    fn setup_handlers(&self) {
        let imp = imp::DockPopover::from_instance(self);
        let dock_object = imp.dock_object.borrow();
        let launch_new_item = imp.launch_new_item.borrow();
        let favorite_item = imp.favorite_item.borrow();
        let quit_all_item = imp.quit_all_item.borrow();
        let window_listbox = imp.window_list.borrow();
        // let all_windows_header = imp.all_windows_item_header.borrow();
        // let revealer = &imp.all_windows_item_revealer;

        if let Some(dock_object) = dock_object.as_ref() {
            // println!("setting up popover menu handlers");
            let self_ = self.clone();
            launch_new_item.connect_clicked(glib::clone!(@weak dock_object, => move |_| {
                let app_info = dock_object.property::<Option<DesktopAppInfo>>("appinfo").expect("Failed to convert value to DesktopAppInfo");

                let context = gdk::Display::default().unwrap().app_launch_context();
                if let Err(err) = app_info.launch(&[], Some(&context)) {
                    eprintln!("{}", err);
                }
                self_.emit_hide();
            }));

            let tx = imp.tx.get().unwrap().clone();
            let self_ = self.clone();
            quit_all_item.connect_clicked(glib::clone!(@weak dock_object => move |_| {
                let active = dock_object.property::<BoxedWindowList>("active").0;
                for w in active {
                    let entity = w.entity;
                    let tx = tx.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let _ = tx.clone().send(Event::Close(entity)).await;
                    });
                }
                self_.emit_hide();
            }));

            let tx = imp.tx.get().unwrap().clone();
            let self_ = self.clone();
            favorite_item.connect_clicked(glib::clone!(@weak dock_object => move |_| {
                let saved = dock_object.property::<bool>("saved");
                let tx = tx.clone();
                glib::MainContext::default().spawn_local(async move {
                    if let Some(name) = dock_object.get_name() {
                        let _ = tx.clone().send(Event::Favorite((name, !saved))).await;
                    }
                });
                self_.emit_hide();
            }));

            // all_windows_header.connect_clicked(
            //     glib::clone!(@weak dock_object, @weak revealer => move |self_| {
            //         // dbg!(dock_object);
            //         let revealer = revealer.borrow();
            //         revealer.set_reveal_child(!revealer.reveals_child())
            //     }),
            // );

            let tx = imp.tx.get().unwrap().clone();
            let self_ = self.clone();
            window_listbox.connect_row_activated(
                glib::clone!(@weak dock_object => move |_, item| {
                    let active = dock_object.property::<BoxedWindowList>("active").0;
                    let entity = active[usize::try_from(item.index()).unwrap()].entity;
                    let tx = tx.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let _ = tx.send(Event::Activate(entity)).await;
                    });
                    self_.emit_hide();
                }),
            );
        }
    }
}
