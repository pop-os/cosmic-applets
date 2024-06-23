// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{iced, widget::icon};
use futures::{FutureExt, StreamExt};
use zbus::zvariant::{self, OwnedValue};

#[derive(Clone, Debug)]
pub struct StatusNotifierItem {
    name: String,
    icon_name: String,
    // TODO Handle icon with multiple sizes?
    icon_pixmap: Option<icon::Handle>,
    _item_proxy: StatusNotifierItemProxy<'static>,
    menu_proxy: DBusMenuProxy<'static>,
}

#[derive(Clone, Debug, zvariant::Value)]
pub struct Icon {
    width: i32,
    height: i32,
    bytes: Vec<u8>,
}

impl StatusNotifierItem {
    pub async fn new(connection: &zbus::Connection, name: String) -> zbus::Result<Self> {
        let (dest, path) = if let Some(idx) = name.find('/') {
            (&name[..idx], &name[idx..])
        } else {
            (name.as_str(), "/StatusNotifierItem")
        };

        let item_proxy = StatusNotifierItemProxy::builder(connection)
            .destination(dest.to_string())?
            .path(path.to_string())?
            .build()
            .await?;

        let icon_name = item_proxy.icon_name().await.unwrap_or_default();
        let icon_pixmap = item_proxy
            .icon_pixmap()
            .await
            .unwrap_or_default()
            .into_iter()
            .max_by_key(|i| (i.width, i.height))
            .map(|mut i| {
                // Convert ARGB to RGBA
                for pixel in i.bytes.chunks_exact_mut(4) {
                    pixel.rotate_left(1);
                }
                icon::from_raster_pixels(i.width as u32, i.height as u32, i.bytes)
            });

        let menu_path = item_proxy.menu().await?;
        let menu_proxy = DBusMenuProxy::builder(connection)
            .destination(dest.to_string())?
            .path(menu_path)?
            .build()
            .await?;

        Ok(Self {
            name,
            icon_name,
            icon_pixmap,
            _item_proxy: item_proxy,
            menu_proxy,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn icon_name(&self) -> &str {
        &self.icon_name
    }

    pub fn icon_pixmap(&self) -> Option<&icon::Handle> {
        self.icon_pixmap.as_ref()
    }

    // TODO: Only fetch changed part of layout, if that's any faster
    pub fn layout_subscription(&self) -> iced::Subscription<Result<Layout, String>> {
        let menu_proxy = self.menu_proxy.clone();
        iced::subscription::run_with_id(
            format!("status-notifier-item-{}", &self.name),
            async move {
                let initial = futures::stream::once(get_layout(menu_proxy.clone()));
                let layout_updated_stream = menu_proxy.receive_layout_updated().await.unwrap();
                let updates = layout_updated_stream.then(move |_| get_layout(menu_proxy.clone()));
                initial.chain(updates)
            }
            .flatten_stream(),
        )
    }

    pub fn menu_proxy(&self) -> &DBusMenuProxy<'static> {
        &self.menu_proxy
    }
}

async fn get_layout(menu_proxy: DBusMenuProxy<'static>) -> Result<Layout, String> {
    match menu_proxy.get_layout(0, -1, &[]).await {
        Ok((_, layout)) => Ok(layout),
        Err(err) => Err(err.to_string()),
    }
}

#[zbus::proxy(interface = "org.kde.StatusNotifierItem")]
trait StatusNotifierItem {
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;

    // https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/Icons
    #[zbus(property)]
    fn icon_pixmap(&self) -> zbus::Result<Vec<Icon>>;

    #[zbus(property)]
    fn menu(&self) -> zbus::Result<zvariant::OwnedObjectPath>;
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, zvariant::DeserializeDict)]
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
    #[zvariant(rename = "icon-name")]
    icon_name: Option<String>,
    disposition: Option<String>,
    // If this field has a different type, this causes the whole type to fail
    // to parse, due to a zvariant bug.
    // https://github.com/dbus2/zbus/issues/856
    // shortcut: Option<String>,
}

impl zvariant::Type for LayoutProps {
    fn signature() -> zvariant::Signature<'static> {
        zvariant::Signature::try_from("a{sv}").unwrap()
    }
}

#[allow(dead_code)]
impl Layout {
    pub fn id(&self) -> i32 {
        self.0
    }

    pub fn children(&self) -> &[Self] {
        &self.2
    }

    pub fn accessible_desc(&self) -> Option<&str> {
        self.1.accessible_desc.as_deref()
    }

    pub fn children_display(&self) -> Option<&str> {
        self.1.children_display.as_deref()
    }

    pub fn label(&self) -> Option<&str> {
        self.1.label.as_deref()
    }

    pub fn enabled(&self) -> bool {
        self.1.enabled.unwrap_or(true)
    }

    pub fn visible(&self) -> bool {
        self.1.visible.unwrap_or(true)
    }

    pub fn type_(&self) -> Option<&str> {
        self.1.type_.as_deref()
    }

    pub fn toggle_type(&self) -> Option<&str> {
        self.1.toggle_type.as_deref()
    }

    pub fn toggle_state(&self) -> Option<i32> {
        self.1.toggle_state
    }

    pub fn icon_data(&self) -> Option<&[u8]> {
        self.1.icon_data.as_deref()
    }

    pub fn icon_name(&self) -> Option<&str> {
        self.1.icon_name.as_deref()
    }

    pub fn disposition(&self) -> Option<&str> {
        self.1.disposition.as_deref()
    }
}

#[zbus::proxy(interface = "com.canonical.dbusmenu")]
trait DBusMenu {
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: &[&str],
    ) -> zbus::Result<(u32, Layout)>;

    fn event(&self, id: i32, event_id: &str, data: &OwnedValue, timestamp: u32)
        -> zbus::Result<()>;

    fn about_to_show(&self, id: i32) -> zbus::Result<bool>;

    #[zbus(signal)]
    fn layout_updated(&self, revision: u32, parent: i32) -> zbus::Result<()>;
}
