use cascade::cascade;
use futures::stream::StreamExt;
use gtk4::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::unsync::OnceCell;
use std::{cell::RefCell, collections::HashMap};
use zbus::dbus_proxy;

use crate::deref_cell::DerefCell;
use crate::status_menu::StatusMenu;

#[derive(Default)]
pub struct StatusAreaInner {
    box_: DerefCell<gtk4::Box>,
    watcher: OnceCell<StatusNotifierWatcherProxy<'static>>,
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
            async {
                let connection = zbus::Connection::session().await?;
                let watcher = StatusNotifierWatcherProxy::new(&connection).await?;

                let name = connection.unique_name().unwrap().as_str();
                if let Err(err) = watcher.register_status_notifier_host(name).await {
                    eprintln!("Failed to register status notifier host: {}", err);
                }

                let mut registered_stream = watcher.receive_status_notifier_item_registered().await?;
                let mut unregistered_stream = watcher.receive_status_notifier_item_unregistered().await?;

                for name in watcher.registered_status_notifier_items().await? {
                    glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
                        obj.item_registered(&name).await;
                    }));
                }

                glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
                    if let Some(evt) = registered_stream.next().await {
                        if let Ok(args) = evt.args() {
                            obj.item_registered(&args.name).await;
                        }
                    }
                }));

                glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
                    if let Some(evt) = unregistered_stream.next().await {
                        if let Ok(args) = evt.args() {
                            obj.item_unregistered(&args.name);
                        }
                    }
                }));

                let _ = obj.inner().watcher.set(watcher);

                Ok::<_, zbus::Error>(())
            }.await.unwrap_or_else(|err| {
                eprintln!("Failed to connect to 'org.kde.StatusNotifierWatcher': {}", err);
            });
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

#[dbus_proxy(
    interface = "org.kde.StatusNotifierWatcher",
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher"
)]
trait StatusNotifierWatcher {
    fn register_status_notifier_host(&self, name: &str) -> zbus::Result<()>;

    #[dbus_proxy(property)]
    fn registered_status_notifier_items(&self) -> zbus::Result<Vec<String>>;

    #[dbus_proxy(signal)]
    fn status_notifier_item_registered(&self, name: &str) -> zbus::Result<()>;

    #[dbus_proxy(signal)]
    fn status_notifier_item_unregistered(&self, name: &str) -> zbus::Result<()>;
}
