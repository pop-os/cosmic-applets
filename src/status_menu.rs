use byte_string::ByteStr;
use cascade::cascade;
use gtk4::{
    gdk_pixbuf, gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use std::{borrow::Cow, collections::HashMap, fmt, io};

use crate::deref_cell::DerefCell;

#[derive(Default)]
pub struct StatusMenuInner {
    menu_button: DerefCell<gtk4::MenuButton>,
    vbox: DerefCell<gtk4::Box>,
    item: DerefCell<StatusNotifierItem>,
    layout: DerefCell<Layout>,
    dbus_menu: DerefCell<DBusMenu>,
}

#[glib::object_subclass]
impl ObjectSubclass for StatusMenuInner {
    const NAME: &'static str = "S76StatusMenu";
    type ParentType = gtk4::Widget;
    type Type = StatusMenu;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for StatusMenuInner {
    fn constructed(&self, obj: &StatusMenu) {
        let vbox = cascade! {
            gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        };

        let menu_button = cascade! {
            gtk4::MenuButton::new();
            ..set_parent(obj);
            ..set_popover(Some(&cascade! {
                gtk4::Popover::new();
                ..set_child(Some(&vbox));
            }));
        };

        self.menu_button.set(menu_button);
        self.vbox.set(vbox);
    }

    fn dispose(&self, _obj: &StatusMenu) {
        self.menu_button.unparent();
    }
}

impl WidgetImpl for StatusMenuInner {}

glib::wrapper! {
    pub struct StatusMenu(ObjectSubclass<StatusMenuInner>)
        @extends gtk4::Widget;
}

impl StatusMenu {
    pub async fn new(name: &str) -> Result<Self, glib::Error> {
        let item = StatusNotifierItem::new(name).await?;
        let obj = glib::Object::new::<Self>(&[]).unwrap();
        if let Some(icon_name) = item.icon_name().as_deref() {
            obj.inner().menu_button.set_icon_name(&icon_name);
        }

        let menu = item.menu().unwrap(); // XXX unwrap?
        let layout = menu.get_layout(0, -1, &[]).await?.1;

        obj.inner().item.set(item);
        obj.inner().layout.set(layout);
        obj.inner().dbus_menu.set(menu);

        println!("{:#?}", &*obj.inner().layout);
        obj.populate_menu(&obj.inner().vbox, &obj.inner().layout);

        Ok(obj)
    }

    fn inner(&self) -> &StatusMenuInner {
        StatusMenuInner::from_instance(self)
    }

    fn populate_menu(&self, box_: &gtk4::Box, layout: &Layout) {
        for i in layout.children() {
            if i.type_().as_deref() == Some("separator") {
                let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
                box_.append(&separator);
            } else if let Some(label) = i.label() {
                let label_widget = cascade! {
                    gtk4::Label::new(Some(&label));
                    ..set_halign(gtk4::Align::Start);
                    ..set_hexpand(true);
                    ..set_use_underline(true);
                };

                let hbox = cascade! {
                    gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                    ..append(&label_widget);
                };

                if let Some(icon_data) = i.icon_data() {
                    let icon_data = io::Cursor::new(icon_data);
                    let pixbuf = gdk_pixbuf::Pixbuf::from_read(icon_data).unwrap(); // XXX unwrap
                    let image = cascade! {
                        gtk4::Image::from_pixbuf(Some(&pixbuf));
                        ..set_halign(gtk4::Align::End);
                    };
                    hbox.append(&image);
                }

                let id = i.id();
                let button = cascade! {
                    gtk4::Button::new();
                    ..set_child(Some(&hbox));
                    ..style_context().add_class("flat");
                    ..set_sensitive(i.enabled().unwrap_or(true)); // default to true?
                    ..connect_clicked(clone!(@weak self as self_ => move |_| {
                            // XXX data, timestamp
                            self_.inner().dbus_menu.event(id, "clicked", &0.to_variant(), 0);
                    }));
                };
                box_.append(&button);

                if i.children_display().as_deref() == Some("submenu") {
                    let vbox = cascade! {
                        gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                    };

                    let revealer = cascade! {
                        gtk4::Revealer::new();
                        ..set_child(Some(&vbox));
                    };

                    self.populate_menu(&vbox, &i);

                    box_.append(&revealer);

                    button.connect_clicked(move |_| {
                        revealer.set_reveal_child(!revealer.reveals_child());
                    });
                }
            }
        }
    }
}

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

    fn id(&self) -> i32 {
        self.0
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

    fn children(&self) -> &[Self] {
        &self.2
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

    fn event(&self, id: i32, eventid: &str, data: &glib::Variant, timestamp: u32) {
        let res = self.0.call_future(
            "Event",
            Some(&(id, eventid, data, timestamp).to_variant()),
            gio::DBusCallFlags::NONE,
            1000,
        );
        glib::MainContext::default().spawn_local(async move {
            if let Err(err) = res.await {
                eprintln!("Failed to call `Event`: {}", err);
            }
        });
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
