// SPDX-License-Identifier: MPL-2.0-only

use crate::fl;
use cascade::cascade;
use cosmic_panel_config::config::{CosmicPanelConfig, XdgWrapperConfig};
use gtk4::{
    gio::{self, DesktopAppInfo, Icon},
    glib::{self, Object},
    prelude::*,
    Align, Application, Button, Orientation,
};
use std::process::Command;

mod imp;

glib::wrapper! {
    pub struct CosmicPanelAppButtonWindow(ObjectSubclass<imp::CosmicPanelAppButtonWindow>)
        @extends gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl CosmicPanelAppButtonWindow {
    pub fn new(app: &Application, app_desktop_file_name: &str) -> Self {
        let self_: Self = Object::new(&[("application", app)])
            .expect("Failed to create `CosmicPanelButtonWindow`.");
        cascade! {
            &self_;
            ..set_width_request(1);
            ..set_height_request(1);
            ..set_decorated(false);
            ..set_resizable(false);
            ..set_title(Some(app_desktop_file_name));
            ..add_css_class("root_window");
        };

        if let Some(apps_desktop_info) =
            DesktopAppInfo::new(&format!("{}.desktop", app_desktop_file_name))
        {
            let app_button = cascade! {
                Button::new();
                ..add_css_class("apps");
            };
            let config = CosmicPanelConfig::load_from_env().unwrap_or_default();
            let icon = apps_desktop_info.icon().unwrap_or_else(|| {
                Icon::for_string("image-missing").expect("Failed to set default icon")
            });
            let container = gtk4::Box::new(Orientation::Horizontal, 0);
            let image = cascade! {
                gtk4::Image::from_gicon(&icon);
                ..set_hexpand(true);
                ..set_halign(Align::Center);
                ..set_pixel_size(config.get_applet_icon_size().try_into().unwrap());
                ..set_tooltip_text(Some(&apps_desktop_info.name()));
            };
            container.append(&image);

            app_button.set_child(Some(&container));
            let app_id = app_desktop_file_name.to_string();
            app_button.connect_clicked(move |_| {
                let _ = Command::new("xdg-shell-wrapper")
                    .env_remove("WAYLAND_SOCKET")
                    .arg(&app_id)
                    .spawn();
            });
            self_.set_child(Some(&app_button));
        } else {
            panic!("{} is not installed", app_desktop_file_name);
        }

        self_
    }
}
