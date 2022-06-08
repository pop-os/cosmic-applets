// SPDX-License-Identifier: MPL-2.0-only

use gtk4::{gdk::Display, gio::{self, ApplicationFlags}, glib, prelude::*, CssProvider, StyleContext};
use once_cell::sync::OnceCell;
use window::CosmicWorkspacesWindow;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use utils::{Event, BoxedWorkspaceList};

mod window;
mod workspace_button;
mod workspace_list;
mod wayland;
mod localize;
mod utils;
mod workspace_object;

const ID: &str = "com.system76.CosmicAppletWorkspaces";
static TX: OnceCell<mpsc::Sender<Event>> = OnceCell::new();

pub fn localize() {
    let localizer = crate::localize::localizer();
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    if let Err(error) = localizer.select(&requested_languages) {
        eprintln!("Error while loading language for App List {}", error);
    }
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_resource("/com.system76.CosmicAppletWorkspaces/style.css");

    StyleContext::add_provider_for_display(
        &Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn main() {
    // Initialize logger
    pretty_env_logger::init();
    glib::set_application_name(ID);

    localize();
    gio::resources_register_include!("compiled.gresource").unwrap();

    let app = gtk4::Application::new(Some(ID), ApplicationFlags::default());

    app.connect_activate(|app| {
        load_css();
        let (tx, mut rx) = mpsc::channel(100);

        let window = CosmicWorkspacesWindow::new(app, tx.clone());

        let workspace_list = Arc::new(Mutex::new(Vec::<BoxedWorkspaceList>::new()));

        TX.set(tx.clone()).unwrap();

        let _ = glib::MainContext::default().spawn_local(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    Event::Activate(_) => {
                        // TODO activate the selected workspace
                    }
                    Event::WorkspaceList => {
                        // TODO update the model with the new workspace list
                    }
                }
            }
        });
        window.show();
    });
    app.run();
}
