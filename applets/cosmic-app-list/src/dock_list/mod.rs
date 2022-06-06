// SPDX-License-Identifier: MPL-2.0-only

use crate::dock_item::DockItem;
use crate::dock_object::DockObject;
use crate::utils::data_path;
use crate::utils::{BoxedWindowList, Event, Item};
use cascade::cascade;
use cosmic_panel_config::config::{Anchor, CosmicPanelConfig};
use gio::DesktopAppInfo;
use gio::Icon;
use glib::Object;
use glib::Type;
use gtk4::gdk;
use gtk4::gdk::ContentProvider;
use gtk4::gdk::Display;
use gtk4::gdk::ModifierType;
use gtk4::glib;
use gtk4::prelude::ListModelExt;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::DropTarget;
use gtk4::IconTheme;
use gtk4::ListView;
use gtk4::Orientation;
use gtk4::SignalListItemFactory;
use gtk4::{DragSource, GestureClick};
use std::fs::File;
use std::path::Path;
use tokio::sync::mpsc::Sender;

mod imp;

glib::wrapper! {
    pub struct DockList(ObjectSubclass<imp::DockList>)
        @extends gtk4::Widget, gtk4::Box,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum DockListType {
    Saved,
    Active,
}

impl Default for DockListType {
    fn default() -> Self {
        DockListType::Active
    }
}

impl DockList {
    pub fn new(type_: DockListType, tx: Sender<Event>, config: CosmicPanelConfig) -> Self {
        let self_: DockList = glib::Object::new(&[]).expect("Failed to create DockList");
        let imp = imp::DockList::from_instance(&self_);
        imp.type_.set(type_).unwrap();
        imp.tx.set(tx).unwrap();
        imp.config.set(config).unwrap();
        self_.layout();
        //dnd behavior is different for each type, as well as the data in the model
        self_.setup_model();
        self_.setup_click_controller();
        self_.setup_drag();
        self_.setup_drop_target();
        self_.setup_factory();
        self_
    }

    pub fn model(&self) -> &gio::ListStore {
        // Get state
        let imp = imp::DockList::from_instance(self);
        imp.model.get().expect("Could not get model")
    }

    pub fn drop_controller(&self) -> &DropTarget {
        // Get state
        let imp = imp::DockList::from_instance(self);
        imp.drop_controller.get().expect("Could not get model")
    }

    pub fn popover_index(&self) -> Option<u32> {
        // Get state
        let imp = imp::DockList::from_instance(self);
        imp.popover_menu_index.get()
    }

    fn restore_data(&self) {
        if let Ok(file) = File::open(data_path()) {
            if let Ok(data) = serde_json::from_reader::<_, Vec<String>>(file) {
                // dbg!(&data);
                let dock_objects: Vec<Object> = data
                    .into_iter()
                    .filter_map(|d| {
                        DockObject::from_app_info_path(&d)
                            .map(|dockobject| dockobject.upcast::<Object>())
                    })
                    .collect();
                // dbg!(&dock_objects);

                let model = self.model();
                model.splice(model.n_items(), 0, &dock_objects);
            }
        } else {
            eprintln!("Error loading saved apps!");
            let model = &self.model();
            xdg::BaseDirectories::new()
                .expect("could not access XDG Base directory")
                .get_data_dirs()
                .iter_mut()
                .for_each(|xdg_data_path| {
                    let defaults = ["Firefox Web Browser", "Files", "Terminal", "Pop!_Shop"];
                    xdg_data_path.push("applications");
                    // dbg!(&xdg_data_path);
                    if let Ok(dir_iter) = std::fs::read_dir(xdg_data_path) {
                        dir_iter.for_each(|dir_entry| {
                            if let Ok(dir_entry) = dir_entry {
                                if let Some(path) = dir_entry.path().file_name() {
                                    if let Some(path) = path.to_str() {
                                        if let Some(app_info) = gio::DesktopAppInfo::new(path) {
                                            if app_info.should_show()
                                                && defaults.contains(&app_info.name().as_str())
                                            {
                                                model.append(&DockObject::new(app_info));
                                            } else {
                                                // println!("Ignoring {}", path);
                                            }
                                        } else {
                                            // println!("error loading {}", path);
                                        }
                                    }
                                }
                            }
                        })
                    }
                });
        }
    }

    fn store_data(model: &gio::ListStore) {
        // Store todo data in vector
        let mut backup_data = Vec::new();
        let mut i = 0;
        while let Some(item) = model.item(i) {
            // Get `AppGroup` from `glib::Object`
            let dock_object = item
                .downcast_ref::<DockObject>()
                .expect("The object needs to be of type `AppGroupData`.");
            // Add todo data to vector and increase position
            if let Some(app_info) = dock_object.property::<Option<DesktopAppInfo>>("appinfo") {
                if let Some(f) = app_info.filename() {
                    backup_data.push(f);
                }
            }
            i += 1;
        }
        // dbg!(&backup_data);
        // Save state in file
        let file = File::create(data_path()).expect("Could not create json file.");
        serde_json::to_writer_pretty(file, &backup_data)
            .expect("Could not write data to json file");
        // TODO save plugins here for now examples are hardcoded and don't need to be saved
    }

    fn layout(&self) {
        let imp = imp::DockList::from_instance(self);
        let list_view = cascade! {
            ListView::default();
            ..set_orientation(Orientation::Horizontal);
            ..add_css_class("transparent");
        };
        if imp.type_.get().unwrap() == &DockListType::Saved {
            list_view.set_width_request(64);
        }
        self.append(&list_view);
        imp.list_view.set(list_view).unwrap();
    }

    pub fn set_position(&self, position: Anchor) {
        let imp = imp::DockList::from_instance(self);
        let model = imp.model.get().unwrap();
        imp.position.replace(position);
        model.items_changed(0, model.n_items(), model.n_items());

        let imp = imp::DockList::from_instance(self);
        imp.list_view
            .get()
            .unwrap()
            .set_orientation(position.into());
    }

    fn setup_model(&self) {
        let imp = imp::DockList::from_instance(self);
        let model = gio::ListStore::new(DockObject::static_type());

        let selection_model = gtk4::NoSelection::new(Some(&model));

        // Wrap model with selection and pass it to the list view
        let list_view = imp.list_view.get().unwrap();
        list_view.set_model(Some(&selection_model));
        imp.model.set(model).expect("Could not set model");

        if imp.type_.get().unwrap() == &DockListType::Saved {
            let model = self.model();
            self.restore_data();
            model.connect_items_changed(|model, _, _removed, _added| {
                Self::store_data(model);
            });
        }
    }

    fn setup_click_controller(&self) {
        let imp = imp::DockList::from_instance(self);
        let controller = GestureClick::builder()
            .button(0)
            .propagation_limit(gtk4::PropagationLimit::None)
            .propagation_phase(gtk4::PropagationPhase::Capture)
            .build();
        self.add_controller(&controller);

        let model = self.model();
        let list_view = &imp.list_view.get().unwrap();
        let popover_menu_index = &imp.popover_menu_index;
        let tx = imp.tx.get().unwrap().clone();
        controller.connect_released(glib::clone!(@weak model, @weak list_view, @weak popover_menu_index => move |self_, _, x, y| {
            let max_x = list_view.allocated_width();
            let max_y = list_view.allocated_height();
            let (indexing_dim, indexing_length, other_dim, other_length) = match list_view.orientation() {
                Orientation::Horizontal => (x, max_x, y, max_y),
                Orientation::Vertical => (y, max_y, x, max_x),
                _ => return,
            };
            // dbg!(max_y);
            // dbg!(y);
            let n_buckets = model.n_items();
            let index = (indexing_dim * n_buckets as f64 / (indexing_length as f64 + 0.1)) as u32;
            // dbg!(self_.current_button());
            // dbg!(self_.last_event(self_.current_sequence().as_ref()));
            let click_modifier = self_.last_event(self_.current_sequence().as_ref()).map(|event| event.modifier_state());
            // dbg!(click_modifier);
            // Launch the application when an item of the list is activated

            let tx = tx.clone();
            let focus_window = move |first_focused_item: &Item| {
                let entity = first_focused_item.entity;
                let tx = tx.clone();
                glib::MainContext::default().spawn_local(async move {
                   let _ = tx.clone().send(Event::Activate(entity)).await;
                });
            };
            let old_index = popover_menu_index.get();
            if let Some(old_index) = old_index  {
                if let Some(old_item) = model.item(old_index) {
                    if let Ok(old_dock_object) = old_item.downcast::<DockObject>() {
                        old_dock_object.set_popover(false);
                        popover_menu_index.replace(None);
                        model.items_changed(old_index, 0, 0);
                        //TODO signal dock to check if it should hide
                    }
                }
                return;
            }
            if other_dim > f64::from(other_length) || y < 0.0 || indexing_dim > f64::from(indexing_length) || indexing_dim < 0.0 {
                // println!("out of bounds click...");
                return;
            }

            if let Some(item) = model.item(index) {
                if let Ok(dock_object) = item.downcast::<DockObject>() {
                    let active = dock_object.property::<BoxedWindowList>("active");
                    let app_info = dock_object.property::<Option<DesktopAppInfo>>("appinfo");
                    match (self_.current_button(), click_modifier, active.0.get(0), app_info) {
                        (click, Some(click_modifier), Some(first_focused_item), _) if click == 1 && !click_modifier.contains(ModifierType::CONTROL_MASK) => focus_window(first_focused_item),
                        (click, None, Some(first_focused_item), _) if click == 1 => focus_window(first_focused_item),
                        (click, _, _, Some(app_info)) | (click, _, None, Some(app_info)) if click != 3  => {
                            let context = gdk::Display::default().unwrap().app_launch_context();
                            if let Err(err) = app_info.launch(&[], Some(&context)) {
                                dbg!(err);
                            }

                        }
                        (click, _, _, _) if click == 3 => {
                            // println!("handling right click");
                            if let Some(old_index) = popover_menu_index.get() {
                                if let Some(item) = model.item(old_index) {
                                    if let Ok(dock_object) = item.downcast::<DockObject>() {
                                        dock_object.set_popover(false);
                                        popover_menu_index.replace(Some(index));
                                        model.items_changed(old_index, 0, 0);
                                    }
                                }
                            }
                            dock_object.set_popover(true);
                            popover_menu_index.replace(Some(index));
                            model.items_changed(index, 0, 0);
                        }
                        _ => eprintln!("Failed to process click.")
                    }
                }
            }
        }));
        imp.click_controller.set(controller).unwrap();
    }

    fn setup_drop_target(&self) {
        let imp = imp::DockList::from_instance(self);
        if imp.type_.get().unwrap() != &DockListType::Saved {
            return;
        }

        let drop_target_widget = &imp.list_view.get().unwrap();
        let mut drop_actions = gdk::DragAction::COPY;
        drop_actions.insert(gdk::DragAction::MOVE);
        let drop_format = gdk::ContentFormats::for_type(Type::STRING);
        let drop_format = drop_format.union(&gdk::ContentFormats::for_type(Type::U32));
        let drop_controller = DropTarget::builder()
            .preload(true)
            .actions(drop_actions)
            .formats(&drop_format)
            .build();
        drop_target_widget.add_controller(&drop_controller);

        let model = self.model();
        let list_view = &imp.list_view.get().unwrap();
        let drag_end = &imp.drag_end_signal;
        let drag_source = &imp.drag_source.get().unwrap();
        let tx = imp.tx.get().unwrap().clone();
        drop_controller.connect_drop(
            glib::clone!(@weak model, @weak list_view, @weak drag_end, @weak drag_source => @default-return true, move |_self, drop_value, x, y| {
                //calculate insertion location
                let max_x = list_view.allocated_width();
                let max_y = list_view.allocated_height();
                let n_buckets = model.n_items() * 2;

                let (indexing_dim, indexing_length, _other_dim, _other_length) = match list_view.orientation() {
                    Orientation::Horizontal => (x, max_x, y, max_y),
                    Orientation::Vertical => (y, max_y, x, max_x),
                    _ => (x, max_x, y, max_y),
                };

                let drop_bucket = (indexing_dim * n_buckets as f64 / (indexing_length as f64 + 0.1)) as u32;
                let index = if drop_bucket == 0 {
                    0
                } else if drop_bucket == n_buckets - 1 {
                    model.n_items()
                } else {
                    (drop_bucket + 1) / 2
                };

                if let Ok(Some(path_str)) = drop_value.get::<Option<String>>() {
                    let desktop_path = &Path::new(&path_str);
                    if let Some(pathbase) = desktop_path.file_name() {
                        if let Some(app_info) = gio::DesktopAppInfo::new(&pathbase.to_string_lossy()) {
                            // remove item if already exists
                            let mut i: u32 = 0;
                            let mut index_of_existing_app: Option<u32> = None;
                            while let Some(item) = model.item(i) {
                                if let Ok(cur_app_info) = item.downcast::<DockObject>() {
                                    if let Some(cur_app_info) = cur_app_info.property::<Option<DesktopAppInfo>>("appinfo") {
                                        if cur_app_info.filename() == Some(Path::new(&path_str).to_path_buf()) {
                                            index_of_existing_app = Some(i);
                                        }
                                    }
                                }
                                i += 1;
                            }
                            if let Some(index_of_existing_app) = index_of_existing_app {
                                // remove existing entry
                                model.remove(index_of_existing_app);
                                if let Some(old_handle) = drag_end.replace(None) {
                                    glib::signal_handler_disconnect(&drag_source, old_handle);
                                }
                            }
                            model.insert(index, &DockObject::new(app_info));
                        }
                    }
                }
                else if let Ok(old_index) = drop_value.get::<u32>() {
                    if let Some(item) = model.item(old_index) {
                        if let Ok(dock_object) = item.downcast::<DockObject>() {
                            model.remove(old_index);
                            model.insert(index, &dock_object);
                            if let Some(old_handle) = drag_end.replace(None) {
                                glib::signal_handler_disconnect(&drag_source, old_handle);
                            }
                        }
                    }
                }
                else {
                    // dbg!("rejecting drop");
                    _self.reject();
                }
                let tx = tx.clone();
                glib::MainContext::default().spawn_local(async move {
                   let _ = tx.send(Event::RefreshFromCache).await;
                });
                true
            }),
        );

        imp.drop_controller
            .set(drop_controller)
            .expect("Could not set dock dnd drop controller");
    }

    fn setup_drag(&self) {
        let imp = imp::DockList::from_instance(self);
        let type_ = imp.type_.get().unwrap();

        let actions = match *type_ {
            DockListType::Saved => gdk::DragAction::MOVE,
            DockListType::Active => gdk::DragAction::COPY,
        };
        let drag_source = DragSource::builder()
            .name("dock drag source")
            .actions(actions)
            .build();

        let model = self.model();
        let list_view = imp.list_view.get().unwrap();
        let drag_end = &imp.drag_end_signal;
        let drag_cancel = &imp.drag_cancel_signal;
        let type_ = *type_;
        let tx = imp.tx.get().unwrap().clone();
        list_view.add_controller(&drag_source);
        drag_source.connect_prepare(glib::clone!(@weak model, @weak list_view, @weak drag_end, @weak drag_cancel => @default-return None, move |self_, x, _y| {
            let max_x = list_view.allocated_width();
            // dbg!(max_x);
            // dbg!(max_y);
            let n_buckets = model.n_items();

            let index = (x * n_buckets as f64 / (max_x as f64 + 0.1)) as u32;
            if let Some(item) = model.item(index) {
                if type_ == DockListType::Saved {
                    let tx1 = tx.clone();
                    if let Some(old_handle) = drag_end.replace(Some(self_.connect_drag_end(
                        glib::clone!(@weak model => move |_self, _drag, _delete_data| {
                            if _delete_data {
                                model.remove(index);
                                let tx = tx1.clone();
                                glib::MainContext::default().spawn_local(async move {
                                    let _ = tx.send(Event::RefreshFromCache).await;
                                });
                            };
                        }),
                    ))) {
                        glib::signal_handler_disconnect(self_, old_handle);
                    }

                    let tx = tx.clone();
                    if let Some(old_handle) = drag_cancel.replace(Some(self_.connect_drag_cancel(
                        glib::clone!(@weak model => @default-return false, move |_self, _drag, cancel_reason| {
                            if cancel_reason != gdk::DragCancelReason::UserCancelled {
                                model.remove(index);
                                let tx = tx.clone();
                                glib::MainContext::default().spawn_local(async move {
                                    let _ = tx.send(Event::RefreshFromCache).await;
                                });
                                true
                            } else  {
                                false
                            }
                        }),
                    ))) {
                        glib::signal_handler_disconnect(self_, old_handle);
                    }
                }
                if let Ok(dock_object) = item.downcast::<DockObject>() {
                    if let Some(app_info) = dock_object.property::<Option<DesktopAppInfo>>("appinfo") {
                        let icon = app_info
                            .icon()
                            .unwrap_or_else(|| Icon::for_string("image-missing").expect("Failed to set default icon"));

                        if let Some(default_display) = &Display::default() {
                            let icon_theme = IconTheme::for_display(default_display);
                            let paintable_icon = icon_theme.lookup_by_gicon(
                                &icon,
                                64,
                                1,
                                gtk4::TextDirection::None,
                                gtk4::IconLookupFlags::empty(),
                            );
                            self_.set_icon(Some(&paintable_icon), 32, 32);
                        }

                        // saved app list provides index
                        return match type_ {
                            DockListType::Saved => Some(ContentProvider::for_value(&index.to_value())),
                            DockListType::Active => app_info.filename().map(|file| ContentProvider::for_value(&file.to_string_lossy().to_value()))
                        }
                    }
                }
            }
            None
        }));

        // TODO investigate why drop does not finish when dropping on some surfaces
        // for now this is a fix that will cancel the drop after 100 ms and not completing.
        drag_source.connect_drag_begin(|_self, drag| {
            drag.connect_drop_performed(|_self| {
                glib::timeout_add_local_once(
                    std::time::Duration::from_millis(100),
                    glib::clone!(@weak _self => move || {
                        _self.drop_done(false);
                    }),
                );
            });
        });

        imp.drag_source
            .set(drag_source)
            .expect("Could not set saved drag source");
    }

    fn setup_factory(&self) {
        let imp = imp::DockList::from_instance(self);
        let popover_menu_index = &imp.popover_menu_index;
        let factory = SignalListItemFactory::new();
        let model = imp.model.get().expect("Failed to get saved app model.");
        let tx = imp.tx.get().unwrap().clone();
        let icon_size = imp.config.get().unwrap().get_applet_icon_size();
        factory.connect_setup(
            glib::clone!(@weak popover_menu_index, @weak model => move |_, list_item| {
                let dock_item = DockItem::new(tx.clone(), icon_size);
                dock_item
                    .connect_local("popover-closed", false, move |_| {
                        if let Some(old_index) = popover_menu_index.replace(None) {
                            if let Some(item) = model.item(old_index) {
                                if let Ok(dock_object) = item.downcast::<DockObject>() {
                                    dock_object.set_popover(false);
                                    model.items_changed(old_index, 0, 0);
                                }
                            }
                        }

                        None
                    });
                list_item.set_child(Some(&dock_item));
            }),
        );
        factory.connect_bind(
            glib::clone!(@weak imp.position as position => move |_, list_item| {
                let dock_object = list_item
                    .item()
                    .expect("The item has to exist.")
                    .downcast::<DockObject>()
                    .expect("The item has to be a `DockObject`");
                let dock_item = list_item
                    .child()
                    .expect("The list item child needs to exist.")
                    .downcast::<DockItem>()
                    .expect("The list item type needs to be `DockItem`");
                dock_item.set_dock_object(&dock_object);
                dock_item.set_position(position.get());
            }),
        );
        // Set the factory of the list view
        imp.list_view.get().unwrap().set_factory(Some(&factory));
    }
}
