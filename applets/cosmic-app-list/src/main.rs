// SPDX-License-Identifier: MPL-2.0-only

use apps_window::CosmicAppListWindow;
use dock_list::DockListType;
use dock_object::DockObject;
use gio::{ApplicationFlags, DesktopAppInfo};
use gtk4::gdk::Display;
use gtk4::{glib, prelude::*, CssProvider, StyleContext};
use once_cell::sync::OnceCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use utils::{block_on, BoxedWindowList, Event, Item, DEST, PATH};

mod apps_container;
mod apps_window;
mod dock_item;
mod dock_list;
mod dock_object;
mod dock_popover;
mod localize;
mod utils;

const ID: &str = "com.system76.CosmicAppList";
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
    glib::set_application_name("Cosmic Dock App List");

    localize();

    gio::resources_register_include!("compiled.gresource").unwrap();
    let app = gtk4::Application::new(Some(ID), ApplicationFlags::default());

    app.connect_activate(|app| {
        load_css();
        let (tx, mut rx) = mpsc::channel(100);

        let window = CosmicAppListWindow::new(app, tx.clone());

        let apps_container = apps_container::AppsContainer::new(tx.clone());
        let cached_results = Arc::new(Mutex::new(Vec::new()));
        // let zbus_conn = spawn_zbus(tx.clone(), Arc::clone(&cached_results));
        TX.set(tx.clone()).unwrap();

        let _ = glib::MainContext::default().spawn_local(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    Event::Activate(_) => {
                        // let _activate_window = zbus_conn
                        //     .call_method(Some(DEST), PATH, Some(DEST), "WindowFocus", &((e,)))
                        //     .await
                        //     .expect("Failed to focus selected window");
                    }
                    Event::Close(_) => {
                        // let _activate_window = zbus_conn
                        //     .call_method(Some(DEST), PATH, Some(DEST), "WindowQuit", &((e,)))
                        //     .await
                        //     .expect("Failed to close selected window");
                    }
                    Event::Favorite((name, should_favorite)) => {
                        let saved_app_model = apps_container.model(DockListType::Saved);
                        let active_app_model = apps_container.model(DockListType::Active);
                        if should_favorite {
                            let mut cur: u32 = 0;
                            let mut index: Option<u32> = None;
                            while let Some(item) = active_app_model.item(cur) {
                                if let Ok(cur_dock_object) = item.downcast::<DockObject>() {
                                    if cur_dock_object.get_path() == Some(name.clone()) {
                                        cur_dock_object.set_saved(true);
                                        index = Some(cur);
                                    }
                                }
                                cur += 1;
                            }
                            if let Some(index) = index {
                                let object = active_app_model.item(index).unwrap();
                                active_app_model.remove(index);
                                saved_app_model.append(&object);
                            }
                        } else {
                            let mut cur: u32 = 0;
                            let mut index: Option<u32> = None;
                            while let Some(item) = saved_app_model.item(cur) {
                                if let Ok(cur_dock_object) = item.downcast::<DockObject>() {
                                    if cur_dock_object.get_path() == Some(name.clone()) {
                                        cur_dock_object.set_saved(false);
                                        index = Some(cur);
                                    }
                                }
                                cur += 1;
                            }
                            if let Some(index) = index {
                                let object = saved_app_model.item(index).unwrap();
                                saved_app_model.remove(index);
                                active_app_model.append(&object);
                            }
                        }
                        let _ = tx.send(Event::RefreshFromCache).await;
                    }
                    Event::RefreshFromCache => {
                        // println!("refreshing model from cache");
                        let cached_results = cached_results.as_ref().lock().unwrap();
                        let stack_active = cached_results.iter().fold(
                            BTreeMap::new(),
                            |mut acc: BTreeMap<String, BoxedWindowList>, elem: &Item| {
                                if let Some(v) = acc.get_mut(&elem.description) {
                                    v.0.push(elem.clone());
                                } else {
                                    acc.insert(
                                        elem.description.clone(),
                                        BoxedWindowList(vec![elem.clone()]),
                                    );
                                }
                                acc
                            },
                        );
                        let mut stack_active: Vec<BoxedWindowList> =
                            stack_active.into_values().collect();

                        // update active app stacks for saved apps into the saved app model
                        // then put the rest in the active app model (which doesn't include saved apps)
                        let saved_app_model = apps_container.model(DockListType::Saved);

                        let mut saved_i: u32 = 0;
                        while let Some(item) = saved_app_model.item(saved_i) {
                            if let Ok(dock_obj) = item.downcast::<DockObject>() {
                                if let Some(cur_app_info) =
                                    dock_obj.property::<Option<DesktopAppInfo>>("appinfo")
                                {
                                    if let Some((i, _s)) = stack_active
                                        .iter()
                                        .enumerate()
                                        .find(|(_i, s)| s.0[0].description == cur_app_info.name())
                                    {
                                        // println!(
                                        //     "found active saved app {} at {}",
                                        //     _s.0[0].name, i
                                        // );
                                        let active = stack_active.remove(i);
                                        dock_obj.set_property("active", active.to_value());
                                        saved_app_model.items_changed(saved_i, 0, 0);
                                    } else if cached_results
                                        .iter()
                                        .any(|s| s.description == cur_app_info.name())
                                    {
                                        dock_obj.set_property(
                                            "active",
                                            BoxedWindowList(Vec::new()).to_value(),
                                        );
                                        saved_app_model.items_changed(saved_i, 0, 0);
                                    }
                                }
                            }
                            saved_i += 1;
                        }

                        let active_app_model = apps_container.model(DockListType::Active);
                        let model_len = active_app_model.n_items();
                        let new_results: Vec<glib::Object> = stack_active
                            .into_iter()
                            .map(|v| DockObject::from_search_results(v).upcast())
                            .collect();
                        active_app_model.splice(0, model_len, &new_results[..]);
                    }
                    Event::WindowList => {
                        // sort to make comparison with cache easier
                        let results = cached_results.as_ref().lock().unwrap();

                        // build active app stacks for each app
                        let stack_active = results.iter().fold(
                            BTreeMap::new(),
                            |mut acc: BTreeMap<String, BoxedWindowList>, elem| {
                                if let Some(v) = acc.get_mut(&elem.description) {
                                    v.0.push(elem.clone());
                                } else {
                                    acc.insert(
                                        elem.description.clone(),
                                        BoxedWindowList(vec![elem.clone()]),
                                    );
                                }
                                acc
                            },
                        );
                        let mut stack_active: Vec<BoxedWindowList> =
                            stack_active.into_values().collect();

                        // update active app stacks for saved apps into the saved app model
                        // then put the rest in the active app model (which doesn't include saved apps)
                        let saved_app_model = apps_container.model(DockListType::Saved);

                        let mut saved_i: u32 = 0;
                        while let Some(item) = saved_app_model.item(saved_i) {
                            if let Ok(dock_obj) = item.downcast::<DockObject>() {
                                if let Some(cur_app_info) =
                                    dock_obj.property::<Option<DesktopAppInfo>>("appinfo")
                                {
                                    if let Some((i, _s)) = stack_active
                                        .iter()
                                        .enumerate()
                                        .find(|(_i, s)| s.0[0].description == cur_app_info.name())
                                    {
                                        // println!("found active saved app {} at {}", s.0[0].name, i);
                                        let active = stack_active.remove(i);
                                        dock_obj.set_property("active", active.to_value());
                                        saved_app_model.items_changed(saved_i, 0, 0);
                                    } else if results
                                        .iter()
                                        .any(|s| s.description == cur_app_info.name())
                                    {
                                        dock_obj.set_property(
                                            "active",
                                            BoxedWindowList(Vec::new()).to_value(),
                                        );
                                        saved_app_model.items_changed(saved_i, 0, 0);
                                    }
                                }
                            }
                            saved_i += 1;
                        }

                        let active_app_model = apps_container.model(DockListType::Active);
                        let model_len = active_app_model.n_items();
                        let new_results: Vec<glib::Object> = stack_active
                            .into_iter()
                            .map(|v| DockObject::from_search_results(v).upcast())
                            .collect();
                        active_app_model.splice(0, model_len, &new_results[..]);
                    }
                }
            }
        });
        window.show();
    });
    app.run();
}
