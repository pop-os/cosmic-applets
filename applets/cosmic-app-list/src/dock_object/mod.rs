// SPDX-License-Identifier: MPL-2.0-only

use crate::utils::BoxedWindowList;
use gio::{DesktopAppInfo, Icon};
use gtk4::gdk::glib::Object;
use gtk4::gdk::subclass::prelude::ObjectSubclassExt;
use gtk4::prelude::*;
use gtk4::{glib, Image};
use std::path::Path;

mod imp;

glib::wrapper! {
    pub struct DockObject(ObjectSubclass<imp::DockObject>);
}

impl DockObject {
    pub fn new(appinfo: DesktopAppInfo) -> Self {
        Object::new(&[("appinfo", &Some(appinfo)), ("saved", &true)])
            .expect("Failed to create `DockObject`.")
    }

    pub fn from_app_info_path(path: &str) -> Option<Self> {
        if let Some(path) = Path::new(path).file_name() {
            if let Some(path) = path.to_str() {
                if let Some(appinfo) = gio::DesktopAppInfo::new(path) {
                    if appinfo.should_show() {
                        return Some(
                            Object::new(&[("appinfo", &Some(appinfo)), ("saved", &true)])
                                .expect("Failed to create `DockObject`."),
                        );
                    }
                }
            }
        }
        None
    }

    pub fn get_path(&self) -> Option<String> {
        let imp = imp::DockObject::from_instance(self);
        if let Some(app_info) = imp.appinfo.borrow().as_ref() {
            app_info
                .filename()
                .map(|name| name.to_string_lossy().into())
        } else {
            None
        }
    }

    pub fn get_name(&self) -> Option<String> {
        let imp = imp::DockObject::from_instance(self);
        imp.appinfo.borrow().as_ref().map(|app_info| app_info.name().to_string())
    }

    pub fn get_image(&self) -> gtk4::Image {
        let imp = imp::DockObject::from_instance(self);
        if let Some(app_info) = imp.appinfo.borrow().as_ref() {
            let image = Image::new();
            let icon = app_info
                .icon()
                .unwrap_or_else(|| Icon::for_string("image-missing").expect("Failed to set default icon"));
            image.set_from_gicon(&icon);
            image.set_tooltip_text(None);
            image
        } else {
            eprintln!("failed to load image");
            let image = Image::new();
            image.set_tooltip_text(None);
            image
        }
    }

    pub fn set_saved(&self, is_saved: bool) {
        let imp = imp::DockObject::from_instance(self);
        imp.saved.replace(is_saved);
    }

    pub fn from_search_results(results: BoxedWindowList) -> Self {
        let appinfo = if let Some(first) = results.0.get(0) {
            xdg::BaseDirectories::new()
                .expect("could not access XDG Base directory")
                .get_data_dirs()
                .iter_mut()
                .filter_map(|xdg_data_path| {
                    xdg_data_path.push("applications");
                    std::fs::read_dir(xdg_data_path).ok()
                })
                .flatten()
                .filter_map(|dir_entry| {
                    if let Ok(dir_entry) = dir_entry {
                        if let Some(path) = dir_entry.path().file_name() {
                            if let Some(path) = path.to_str() {
                                if let Some(app_info) = gio::DesktopAppInfo::new(path) {
                                    if app_info.should_show()
                                        && first.description.as_str() == app_info.name().as_str()
                                    {
                                        return Some(app_info);
                                    }
                                }
                            }
                        }
                    }
                    None
                })
                .next()
        } else {
            None
        };
        // dbg!(&appinfo);
        Object::new(&[("appinfo", &appinfo), ("active", &results)])
            .expect("Failed to create `DockObject`.")
    }

    pub fn set_popover(&self, b: bool) {
        let imp = imp::DockObject::from_instance(self);
        imp.popover.replace(b);
    }
}

#[derive(Clone, Debug, Default, glib::Boxed)]
#[boxed_type(name = "BoxedDockObject")]
pub struct BoxedDockObject(pub Option<DockObject>);
