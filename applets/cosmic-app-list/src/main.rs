// SPDX-License-Identifier: MPL-2.0-only

use apps_window::CosmicAppListWindow;
use calloop::channel::SyncSender;
use dock_list::DockListType;
use dock_object::DockObject;
use gio::{ApplicationFlags, DesktopAppInfo};
use gtk4::gdk::Display;
use gtk4::{glib, prelude::*, CssProvider, StyleContext};
use once_cell::sync::OnceCell;
use std::collections::BTreeMap;
use utils::{block_on, AppListEvent, BoxedWindowList, DEST, PATH};
use wayland::{Toplevel, ToplevelEvent};

mod apps_container;
mod apps_window;
mod config;
mod dock_item;
mod dock_list;
mod dock_object;
mod dock_popover;
mod localize;
mod utils;
mod wayland;
mod wayland_source;

const ID: &str = "com.system76.CosmicAppList";
static TX: OnceCell<glib::Sender<AppListEvent>> = OnceCell::new();
static WAYLAND_TX: OnceCell<SyncSender<ToplevelEvent>> = OnceCell::new();

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
        let (tx, rx) = glib::MainContext::channel(glib::Priority::default());

        let window = CosmicAppListWindow::new(app);
        let wayland_tx = wayland::spawn_toplevels();

        WAYLAND_TX.set(wayland_tx).unwrap();



        let mut cached_results = Vec::new();
        // let zbus_conn = spawn_zbus(tx.clone(), Arc::clone(&cached_results));
        TX.set(tx.clone()).unwrap();

        rx.attach(None, glib::clone!(@weak window => @default-return glib::prelude::Continue(true), move |event| {
            let apps_container = window.apps_container();
            let should_apply_changes = match event {
                AppListEvent::Favorite((name, should_favorite)) => {
                    let saved_app_model = apps_container.model(DockListType::Saved);
                    let active_app_model = apps_container.model(DockListType::Active);
                    if should_favorite {
                        let mut cur: u32 = 0;
                        let mut index: Option<u32> = None;
                        while let Some(item) = active_app_model.item(cur) {
                            if let Ok(cur_dock_object) = item.downcast::<DockObject>() {
                                if cur_dock_object.get_name() == Some(name.clone()) {
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
                                if cur_dock_object.get_name() == Some(name.clone()) {
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
                    let _ = tx.send(AppListEvent::Refresh);
                    false
                }
                AppListEvent::Refresh => {
                    // println!("refreshing model from cache");
                    let stack_active = cached_results.iter().fold(
                        BTreeMap::new(),
                        |mut acc: BTreeMap<String, BoxedWindowList>, elem: &Toplevel| {
                            if let Some(v) = acc.get_mut(&elem.app_id) {
                                v.0.push(elem.clone());
                            } else {
                                acc.insert(
                                    elem.app_id.clone(),
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
                                    .find(|(_i, s)| Some(&s.0[0].app_id) == cur_app_info.filename().and_then(|p| p
                                            .file_stem()
                                            .and_then(|s| s.to_str().map(|s| s.to_string()))).as_ref())
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
                                    .any(|s| Some(&s.app_id) == cur_app_info.filename().and_then(|p| p
                                            .file_stem()
                                            .and_then(|s| s.to_str().map(|s| s.to_string()))).as_ref())
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
                        .filter_map(|v| DockObject::from_window_list(v).map(|o| o.upcast()))
                        .collect();
                    active_app_model.splice(0, model_len, &new_results[..]);
                    true
                }
                AppListEvent::WindowList(toplevels) => {
                    cached_results = toplevels;
                    true
                }
                AppListEvent::Remove(top_level) => {
                    dbg!(cached_results.len());
                    if let Some(i) = cached_results.iter().position(|t| t.toplevel_handle == top_level.toplevel_handle) {
                        cached_results.swap_remove(i);
                    }
                    dbg!(cached_results.len());

                    true
                }
                AppListEvent::Add(top_level) => {
                    // sort to make comparison with cache easier
                    if let Some(i) = cached_results.iter().position(|t| t.toplevel_handle == top_level.toplevel_handle) {
                        cached_results[i] = top_level;
                    } else {
                        cached_results.push(top_level);
                    }
                    true
                }
            };
            if should_apply_changes {
                    // dbg!(&cached_results);
                    // build active app stacks for each app
                    let stack_active = cached_results.iter().fold(
                        BTreeMap::new(),
                        |mut acc: BTreeMap<String, BoxedWindowList>, elem| {
                            if let Some(v) = acc.get_mut(&elem.app_id) {
                                v.0.push(elem.clone());
                            } else {
                                acc.insert(
                                    elem.app_id.clone(),
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
                            // clear active if it has some, they will be updated back if they still exist
                            let prev_active: BoxedWindowList = dock_obj.property("active");
                            if !prev_active.0.is_empty() {
                                dock_obj.set_property("active", BoxedWindowList::default().to_value());
                                saved_app_model.items_changed(saved_i, 0, 0);
                            }
                        
                            if let Some(cur_app_info) =
                                dock_obj.property::<Option<DesktopAppInfo>>("appinfo")
                            {
                                if let Some((i, _s)) = stack_active
                                    .iter()
                                    .enumerate()
                                    .find(|(_i, s)| Some(&s.0[0].app_id) == cur_app_info.filename().and_then(|p| p
                                            .file_stem()
                                            .and_then(|s| s.to_str().map(|s| s.to_string()))).as_ref())
                                {
                                    // println!("found active saved app {} at {}", s.0[0].name, i);
                                    let active = stack_active.remove(i);
                                    dock_obj.set_property("active", active.to_value());
                                    saved_app_model.items_changed(saved_i, 0, 0);
                                } else if cached_results
                                    .iter()
                                    .any(|s| Some(&s.app_id) == cur_app_info.filename().and_then(|p| p
                                            .file_stem()
                                            .and_then(|s| s.to_str().map(|s| s.to_string()))).as_ref())
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
                        .filter_map(|v| DockObject::from_window_list(v).map(|o| o.upcast()))
                        .collect();

                    active_app_model.splice(0, model_len, &new_results[..]);
            }
            glib::prelude::Continue(true)
        }));

        window.show();
    });
    app.run();
}
