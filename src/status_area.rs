// TODO
// - Implement StatusNotifierWatcher if one is not running
// - Register with StatusNotiferWatcher
// - Handle signals for registered/unreigisted items

use byte_string::ByteStr;
use cascade::cascade;
use gtk4::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::unsync::OnceCell;
use std::{borrow::Cow, cell::RefCell, collections::HashMap, fmt};

use crate::deref_cell::DerefCell;

#[derive(Default)]
pub struct StatusAreaInner {
    box_: DerefCell<gtk4::Box>,
    watcher: OnceCell<StatusNotifierWatcher>,
    icons: RefCell<HashMap<String, gtk4::Image>>,
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
        match StatusNotifierItem::new(&name).await {
            Ok(item) => {
                let image = gtk4::Image::from_icon_name(item.icon_name().as_deref());
                self.inner().box_.append(&image);

                if let Some(menu) = item.menu() {
                    println!("{:#?}", menu.get_layout(0, -1, &[]).await);
                }

                self.item_unregistered(name);
                self.inner()
                    .icons
                    .borrow_mut()
                    .insert(name.to_owned(), image);
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

//#[derive(Debug)]
struct Layout(i32, HashMap<String, glib::Variant>, Vec<Layout>);

impl fmt::Debug for Layout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = f.debug_struct("Layout");
        s.field("id", &self.0);
        for (k, v) in &self.1 {
            if let Some(v) = v.get::<String>() {
                s.field(k, &v);
            } else if let Some(v) = v.get::<i32>() {
                s.field(k, &v);
            } else if let Some(v) = v.get::<bool>() {
                s.field(k, &v);
            } else if let Some(v) = v.get::<Vec<u8>>() {
                s.field(k, &ByteStr::new(&v));
            } else {
                s.field(k, v);
            }
        }
        s.field("children", &self.2);
        s.finish()
    }
}

#[allow(dead_code)]
impl Layout {
    fn prop<T: glib::FromVariant>(&self, name: &str) -> Option<T> {
        self.1.get(name)?.get()
    }

    fn accessible_desc(&self) -> Option<String> {
        self.prop("accessible-desc")
    }

    fn children_display(&self) -> Option<String> {
        self.prop("children-display")
    }

    fn label(&self) -> Option<String> {
        self.prop("label")
    }

    fn enabled(&self) -> Option<bool> {
        self.prop("enabled")
    }

    fn visible(&self) -> Option<bool> {
        self.prop("visible")
    }

    fn type_(&self) -> Option<String> {
        self.prop("type")
    }

    fn toggle_type(&self) -> Option<String> {
        self.prop("toggle-type")
    }

    fn toggle_state(&self) -> Option<bool> {
        self.prop("toggle-state")
    }

    fn icon_data(&self) -> Option<Vec<u8>> {
        self.prop("icon-data")
    }
}

impl glib::StaticVariantType for Layout {
    fn static_variant_type() -> Cow<'static, glib::VariantTy> {
        glib::VariantTy::new("(ia{sv}av)").unwrap().into()
    }
}

impl glib::FromVariant for Layout {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        let (id, props, children) = variant.get::<(_, _, Vec<glib::Variant>)>()?;
        let children = children.iter().filter_map(Self::from_variant).collect();
        Some(Self(id, props, children))
    }
}

#[derive(Clone)]
struct DBusMenu(gio::DBusProxy);

impl DBusMenu {
    async fn new(dest: &str, path: &str) -> Result<Self, glib::Error> {
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            dest,
            path,
            "com.canonical.dbusmenu",
        )
        .await?;
        Ok(Self(proxy))
    }

    async fn get_layout(
        &self,
        parent: i32,
        depth: i32,
        properties: &[&str],
    ) -> Result<(u32, Layout), glib::Error> {
        // XXX unwrap
        Ok(self
            .0
            .call_future(
                "GetLayout",
                Some(&(parent, depth, properties).to_variant()),
                gio::DBusCallFlags::NONE,
                1000,
            )
            .await?
            .get()
            .unwrap())
    }
}

struct StatusNotifierItem(gio::DBusProxy, Option<DBusMenu>);

impl StatusNotifierItem {
    async fn new(name: &str) -> Result<Self, glib::Error> {
        let idx = name.find('/').unwrap();
        let dest = &name[..idx];
        let path = &name[idx..];
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            dest,
            path,
            "org.kde.StatusNotifierItem",
        )
        .await?;
        let menu_path = proxy
            .cached_property("Menu")
            .and_then(|x| x.get::<String>());
        let menu = if let Some(menu_path) = menu_path {
            Some(DBusMenu::new(dest, &menu_path).await?)
        } else {
            None
        };
        Ok(Self(proxy, menu))
    }

    fn property<T: glib::FromVariant>(&self, prop: &str) -> Option<T> {
        self.0.cached_property(prop)?.get()
    }

    fn icon_name(&self) -> Option<String> {
        // TODO: IconThemePath? AttentionIconName?
        self.property("IconName")
    }

    fn menu(&self) -> Option<DBusMenu> {
        self.1.clone()
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
