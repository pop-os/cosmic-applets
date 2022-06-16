// SPDX-License-Identifier: MPL-2.0-only

use apps_window::CosmicPanelAppButtonWindow;
use gtk4::gdk::Display;
use gtk4::{
    gio::{self, ApplicationFlags},
    glib,
    prelude::*,
    CssProvider, StyleContext,
};
use once_cell::sync::OnceCell;

mod apps_window;
mod localize;
mod utils;

static ID: OnceCell<String> = OnceCell::new();

pub fn localize() {
    let localizer = crate::localize::localizer();
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    if let Err(error) = localizer.select(&requested_languages) {
        eprintln!("Error while loading language for App List {}", error);
    }
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_data(include_bytes!("style.css"));

    StyleContext::add_provider_for_display(
        &Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn main() {
    // Initialize logger
    pretty_env_logger::init();
    glib::set_application_name("Cosmic Panel App Button");

    localize();
    gio::resources_register_include!("compiled.gresource").unwrap();
    let app = gtk4::Application::new(None, ApplicationFlags::default());
    app.add_main_option(
        "id",
        glib::Char::from(b'i'),
        glib::OptionFlags::NONE,
        glib::OptionArg::String,
        "id of the launched application",
        None,
    );
    app.connect_handle_local_options(|_app, args| {
        if let Ok(Some(id)) = args.lookup::<String>("id") {
            ID.set(id).unwrap();
            -1
        } else {
            1
        }
    });
    app.connect_activate(|app| {
        load_css();
        let id = ID.get().unwrap().clone();
        let window = CosmicPanelAppButtonWindow::new(app, &id);

        window.show();
    });
    app.run();
}
