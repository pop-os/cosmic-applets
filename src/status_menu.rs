use cascade::cascade;
use futures::StreamExt;
use gtk4::{
    gdk_pixbuf,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use std::{cell::RefCell, collections::HashMap, io};
use zbus::dbus_proxy;
use zvariant::OwnedValue;

use crate::deref_cell::DerefCell;
use crate::popover_container::PopoverContainer;

struct Menu {
    box_: gtk4::Box,
    children: Vec<i32>,
}

#[derive(Default)]
pub struct StatusMenuInner {
    button: DerefCell<gtk4::ToggleButton>,
    popover_container: DerefCell<PopoverContainer>,
    vbox: DerefCell<gtk4::Box>,
    item: DerefCell<StatusNotifierItemProxy<'static>>,
    dbus_menu: DerefCell<DBusMenuProxy<'static>>,
    menus: RefCell<HashMap<i32, Menu>>,
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

        let button = cascade! {
            gtk4::ToggleButton::new();
            ..set_has_frame(false);
        };

        let popover_container = cascade! {
            PopoverContainer::new(&button);
            ..set_parent(obj);
            ..popover().set_child(Some(&vbox));
            ..popover().bind_property("visible", &button, "active").flags(glib::BindingFlags::BIDIRECTIONAL).build();
        };

        self.button.set(button);
        self.popover_container.set(popover_container);
        self.vbox.set(vbox);
    }

    fn dispose(&self, _obj: &StatusMenu) {
        self.button.unparent();
    }
}

impl WidgetImpl for StatusMenuInner {}

glib::wrapper! {
    pub struct StatusMenu(ObjectSubclass<StatusMenuInner>)
        @extends gtk4::Widget;
}

impl StatusMenu {
    pub async fn new(name: &str) -> zbus::Result<Self> {
        let (dest, path) = if let Some(idx) = name.find('/') {
            (&name[..idx], &name[idx..])
        } else {
            (name, "/StatusNotifierItem")
        };

        let connection = zbus::Connection::session().await?;
        let item = StatusNotifierItemProxy::builder(&connection)
            .destination(dest.to_string())?
            .path(path.to_string())?
            .build()
            .await?;
        let obj = glib::Object::new::<Self>(&[]).unwrap();
        let icon_name = item.icon_name().await?;
        obj.inner().button.set_icon_name(&icon_name);

        let menu = item.menu().await?;
        let menu = DBusMenuProxy::builder(&connection)
            .destination(dest.to_string())?
            .path(menu)?
            .build()
            .await?;
        let layout = menu.get_layout(0, -1, &[]).await?.1;

        let mut layout_updated_stream = menu.receive_layout_updated().await?;
        glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
            while let Some(evt) = layout_updated_stream.next().await {
                let args = match evt.args() {
                    Ok(args) => args,
                    Err(_) => { continue; },
                };
                obj.layout_updated(args.revision, args.parent);
            }
        }));

        obj.inner().item.set(item);
        obj.inner().dbus_menu.set(menu);

        println!("{:#?}", layout);
        obj.populate_menu(&obj.inner().vbox, &layout);

        Ok(obj)
    }

    fn inner(&self) -> &StatusMenuInner {
        StatusMenuInner::from_instance(self)
    }

    fn layout_updated(&self, _revision: u32, parent: i32) {
        let mut menus = self.inner().menus.borrow_mut();

        if let Some(Menu { box_, children }) = menus.remove(&parent) {
            let mut next_child = box_.first_child();
            while let Some(child) = next_child {
                next_child = child.next_sibling();
                box_.remove(&child);
            }

            fn remove_child_menus(menus: &mut HashMap<i32, Menu>, children: Vec<i32>) {
                for i in children {
                    if let Some(menu) = menus.remove(&i) {
                        remove_child_menus(menus, menu.children);
                    }
                }
            }
            remove_child_menus(&mut menus, children);

            glib::MainContext::default().spawn_local(clone!(@weak self as self_ => async move {
                match self_.inner().dbus_menu.get_layout(parent, -1, &[]).await {
                    Ok((_, layout)) => self_.populate_menu(&box_, &layout),
                    Err(err) => eprintln!("Failed to call 'GetLayout': {}", err),
                }
            }));
        }
    }

    fn populate_menu(&self, box_: &gtk4::Box, layout: &Layout) {
        let mut children = Vec::new();

        for i in layout.children() {
            children.push(i.id());

            if i.type_().as_deref() == Some("separator") {
                let separator = cascade! {
                    gtk4::Separator::new(gtk4::Orientation::Horizontal);
                    ..set_visible(i.visible());
                };
                box_.append(&separator);
            } else if let Some(label) = i.label() {
                let mut label = label.to_string();
                if let Some(toggle_state) = i.toggle_state() {
                    if toggle_state != 0 {
                        label = format!("✓ {}", label);
                    }
                }

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
                    let icon_data = io::Cursor::new(icon_data.to_vec());
                    let pixbuf = gdk_pixbuf::Pixbuf::from_read(icon_data).unwrap(); // XXX unwrap
                    let image = cascade! {
                        gtk4::Image::from_pixbuf(Some(&pixbuf));
                        ..set_halign(gtk4::Align::End);
                    };
                    hbox.append(&image);
                }

                let id = i.id();
                let close_on_click = i.children_display().as_deref() != Some("submenu");
                let button = cascade! {
                    gtk4::Button::new();
                    ..set_child(Some(&hbox));
                    ..style_context().add_class("flat");
                    ..set_visible(i.visible());
                    ..set_sensitive(i.enabled());
                    ..connect_clicked(clone!(@weak self as self_ => move |_| {
                            // XXX data, timestamp
                            if close_on_click {
                                self_.inner().popover_container.popdown();
                            }
                            glib::MainContext::default().spawn_local(clone!(@strong self_ => async move {
                                let _ = self_.inner().dbus_menu.event(id, "clicked", &0.into(), 0).await;
                            }));
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

        self.inner().menus.borrow_mut().insert(
            layout.id(),
            Menu {
                box_: box_.clone(),
                children,
            },
        );
    }
}

#[dbus_proxy(interface = "org.kde.StatusNotifierItem")]
trait StatusNotifierItem {
    #[dbus_proxy(property)]
    fn icon_name(&self) -> zbus::Result<String>;

    #[dbus_proxy(property)]
    fn menu(&self) -> zbus::Result<zvariant::OwnedObjectPath>;
}

#[derive(Debug)]
pub struct Layout(i32, LayoutProps, Vec<Layout>);

impl<'a> serde::Deserialize<'a> for Layout {
    fn deserialize<D: serde::Deserializer<'a>>(deserializer: D) -> Result<Self, D::Error> {
        let (id, props, children) =
            <(i32, LayoutProps, Vec<(zvariant::Signature<'_>, Self)>)>::deserialize(deserializer)?;
        Ok(Self(id, props, children.into_iter().map(|x| x.1).collect()))
    }
}

impl zvariant::Type for Layout {
    fn signature() -> zvariant::Signature<'static> {
        zvariant::Signature::try_from("(ia{sv}av)").unwrap()
    }
}

#[derive(Debug, zvariant::DeserializeDict, zvariant::Type)]
pub struct LayoutProps {
    #[zvariant(rename = "accessible-desc")]
    accessible_desc: Option<String>,
    #[zvariant(rename = "children-display")]
    children_display: Option<String>,
    label: Option<String>,
    enabled: Option<bool>,
    visible: Option<bool>,
    #[zvariant(rename = "type")]
    type_: Option<String>,
    #[zvariant(rename = "toggle-type")]
    toggle_type: Option<String>,
    #[zvariant(rename = "toggle-state")]
    toggle_state: Option<i32>,
    #[zvariant(rename = "icon-data")]
    icon_data: Option<Vec<u8>>,
}

#[allow(dead_code)]
impl Layout {
    fn id(&self) -> i32 {
        self.0
    }

    fn children(&self) -> &[Self] {
        &self.2
    }

    fn accessible_desc(&self) -> Option<&str> {
        self.1.accessible_desc.as_deref()
    }

    fn children_display(&self) -> Option<&str> {
        self.1.children_display.as_deref()
    }

    fn label(&self) -> Option<&str> {
        self.1.label.as_deref()
    }

    fn enabled(&self) -> bool {
        self.1.enabled.unwrap_or(true)
    }

    fn visible(&self) -> bool {
        self.1.visible.unwrap_or(true)
    }

    fn type_(&self) -> Option<&str> {
        self.1.type_.as_deref()
    }

    fn toggle_type(&self) -> Option<&str> {
        self.1.toggle_type.as_deref()
    }

    fn toggle_state(&self) -> Option<i32> {
        self.1.toggle_state
    }

    fn icon_data(&self) -> Option<&[u8]> {
        self.1.icon_data.as_deref()
    }
}

#[dbus_proxy(interface = "com.canonical.dbusmenu")]
trait DBusMenu {
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: &[&str],
    ) -> zbus::Result<(u32, Layout)>;

    fn event(&self, id: i32, event_id: &str, data: &OwnedValue, timestamp: u32)
        -> zbus::Result<()>;

    #[dbus_proxy(signal)]
    fn layout_updated(&self, revision: u32, parent: i32) -> zbus::Result<()>;
}
