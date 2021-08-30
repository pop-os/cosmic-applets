// TODO
// - Implement StatusNotifierWatcher if one is not running
// - Register with StatusNotiferWatcher
// - Handle signals for registered/unreigisted items

use cascade::cascade;
use gtk4::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::unsync::OnceCell;
use std::{cell::RefCell, collections::HashMap};

use crate::deref_cell::DerefCell;
use crate::status_menu::StatusMenu;

#[derive(Default)]
pub struct StatusAreaInner {
    box_: DerefCell<gtk4::Box>,
    watcher: OnceCell<StatusNotifierWatcher>,
    icons: RefCell<HashMap<String, StatusMenu>>,
}

#[glib::object_subclass]
impl ObjectSubclass for StatusAreaInner {
    const NAME: &'static str = "S76StatusArea";
    type ParentType = gtk4::Widget;
    type Type = StatusArea;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for StatusAreaInner {
    fn constructed(&self, obj: &StatusArea) {
        let box_ = cascade! {
            gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            ..set_parent(obj);
        };

        self.box_.set(box_);

        glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
            let watcher = match StatusNotifierWatcher::new().await {
                Ok(watcher) => watcher,
                Err(err) => {
                    eprintln!("Failed to connect to 'org.kde.StatusNotifierWatcher': {}", err);
                    return;
                }
            };

            for name in watcher.registered_items().await {
                glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
                    obj.item_registered(&name).await;
                }));
            }

            watcher.connect_item_registered_unregistered(clone!(@strong obj => move |name, registered| {
                if registered {
                    glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
                        obj.item_registered(&name).await;
                    }));
                } else {
                    obj.item_unregistered(&name);
                }
            }));

            let _ = obj.inner().watcher.set(watcher);
        }));
    }

    fn dispose(&self, _obj: &StatusArea) {
        self.box_.unparent();
    }
}

impl WidgetImpl for StatusAreaInner {}

glib::wrapper! {
    pub struct StatusArea(ObjectSubclass<StatusAreaInner>)
        @extends gtk4::Widget;
}

impl StatusArea {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }

    fn inner(&self) -> &StatusAreaInner {
        StatusAreaInner::from_instance(self)
    }

    async fn item_registered(&self, name: &str) {
        match StatusMenu::new(&name).await {
            Ok(item) => {
                self.inner().box_.append(&item);

                self.item_unregistered(name);
                self.inner()
                    .icons
                    .borrow_mut()
                    .insert(name.to_owned(), item);
            }
            Err(err) => eprintln!("Failed to connect to '{}': {}", name, err),
        }
    }

    fn item_unregistered(&self, name: &str) {
        if let Some(icon) = self.inner().icons.borrow_mut().remove(name) {
            self.inner().box_.remove(&icon);
        }
    }
}

struct StatusNotifierWatcher(gio::DBusProxy);

impl StatusNotifierWatcher {
    async fn new() -> Result<Self, glib::Error> {
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            "org.kde.StatusNotifierWatcher",
            "/StatusNotifierWatcher",
            "org.kde.StatusNotifierWatcher",
        )
        .await?;
        Ok(Self(proxy))
    }

    fn property<T: glib::FromVariant>(&self, prop: &str) -> Option<T> {
        self.0.cached_property(prop)?.get()
    }

    async fn registered_items(&self) -> Vec<String> {
        self.property::<Vec<String>>("RegisteredStatusNotifierItems")
            .unwrap_or_default()
    }

    fn connect_item_registered_unregistered<F: Fn(String, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.0
            .connect_local("g-signal", false, move |args| {
                let signal_args = args[3].get::<glib::Variant>().unwrap();
                match args[2].get::<String>().unwrap().as_str() {
                    "StatusNotifierItemRegistered" => {
                        f(signal_args.get::<(String,)>().unwrap().0, true);
                    }
                    "StatusNotifierItemUnregistered" => {
                        f(signal_args.get::<(String,)>().unwrap().0, false);
                    }
                    _ => {}
                }
                None
            })
            .unwrap()
    }
}
