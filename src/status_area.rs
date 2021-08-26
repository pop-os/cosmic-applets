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

use crate::deref_cell::DerefCell;

#[derive(Default)]
pub struct StatusAreaInner {
    box_: DerefCell<gtk4::Box>,
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

            for i in watcher.registered_status_notifier_items().await {
                let image = gtk4::Image::from_icon_name(i.icon_name().as_deref());
                obj.inner().box_.append(&image);
            }
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
}

struct StatusNotifierItem(gio::DBusProxy);

impl StatusNotifierItem {
    async fn new(dest: &str, path: &str) -> Result<Self, glib::Error> {
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            dest,
            path,
            "org.kde.StatusNotifierItem",
        )
        .await?;
        Ok(Self(proxy))
    }

    fn property<T: glib::FromVariant>(&self, prop: &str) -> Option<T> {
        self.0.cached_property(prop)?.get()
    }

    fn icon_name(&self) -> Option<String> {
        // TODO: IconThemePath? AttentionIconName?
        self.property("IconName")
    }

    fn menu(&self) -> Option<String> {
        // TODO: Return menu rather than just string
        self.property("Menu")
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

    async fn registered_status_notifier_items(&self) -> Vec<StatusNotifierItem> {
        let mut items = Vec::new();
        for i in self
            .property::<Vec<String>>("RegisteredStatusNotifierItems")
            .unwrap_or_default()
        {
            let idx = i.find('/').unwrap();
            match StatusNotifierItem::new(&i[..idx], &i[idx..]).await {
                Ok(item) => items.push(item),
                Err(err) => eprintln!("Failed to connect to '{}': {}", i, err),
            }
        }
        items
    }
}
