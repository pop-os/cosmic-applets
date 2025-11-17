// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    fl,
    wayland_subscription::{
        OutputUpdate, ToplevelRequest, ToplevelUpdate, WaylandImage, WaylandRequest, WaylandUpdate,
        wayland_subscription,
    },
};
use cctk::{
    sctk::{output::OutputInfo, reexports::calloop::channel::Sender},
    toplevel_info::ToplevelInfo,
    wayland_client::protocol::{
        wl_data_device_manager::DndAction, wl_output::WlOutput, wl_seat::WlSeat,
    },
    wayland_protocols::ext::{
        foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        workspace::v1::client::ext_workspace_handle_v1::ExtWorkspaceHandleV1,
    },
};
use cosmic::desktop::fde::unicase::Ascii;
use cosmic::desktop::fde::{self, DesktopEntry, get_languages_from_env};
use cosmic::{
    Apply, Element, Task, app,
    applet::{
        Context, Size,
        cosmic_panel_config::{PanelAnchor, PanelSize},
    },
    cosmic_config::{Config, CosmicConfigEntry},
    desktop::IconSourceExt,
    iced::{
        self, Limits, Subscription,
        clipboard::mime::{AllowedMimeTypes, AsMimeTypes},
        event::listen_with,
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        widget::{Column, Row, column, mouse_area, row, vertical_rule, vertical_space},
        window,
    },
    iced_core::{Border, Padding},
    iced_runtime::{core::event, dnd::peek_dnd},
    surface,
    theme::{self, Button, Container},
    widget::{
        DndDestination, Image, button, container, divider, dnd_source, horizontal_space,
        icon::{self, from_name},
        image::Handle,
        rectangle_tracker::{RectangleTracker, RectangleUpdate, rectangle_tracker_subscription},
        svg, text,
    },
};
use cosmic_app_list_config::{APP_ID, AppListConfig};
use cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::State;
use futures::future::pending;
use iced::{Alignment, Background, Length};
use rustc_hash::FxHashMap;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    rc::Rc,
    str::FromStr,
    time::Duration,
};
use switcheroo_control::Gpu;
use tokio::time::sleep;
use url::Url;

static MIME_TYPE: &str = "text/uri-list";

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicAppList>(())
}

#[derive(Debug, Clone)]
struct AppletIconData {
    icon_size: u16,
    icon_spacing: f32,
    dot_radius: f32,
    bar_size: f32,
    padding: Padding,
}

static DND_FAVORITES: u64 = u64::MAX;

fn icon_source_with_flatpak_fallback(icon_name: &str) -> fde::IconSource {
    if !icon_name.is_empty() && !icon_name.starts_with('/') {
        if let Some(flatpak_icon_path) = find_flatpak_appstream_icon(icon_name) {
            return fde::IconSource::from_unknown(&flatpak_icon_path.to_string_lossy());
        }
    }

    fde::IconSource::from_unknown(icon_name)
}

fn try_icon_sizes(hash_path: &Path, icon_name: &str) -> Option<PathBuf> {
    const SIZES: &[&str] = &["128x128", "64x64", "48x48", "32x32", "scalable"];

    for size in SIZES {
        let icon_path = if *size == "scalable" {
            hash_path
                .join("icons")
                .join(*size)
                .join("apps")
                .join(format!("{}.svg", icon_name))
        } else {
            hash_path
                .join("icons")
                .join(*size)
                .join(format!("{}.png", icon_name))
        };

        if icon_path.exists() {
            return Some(icon_path);
        }
    }

    None
}

fn find_flatpak_appstream_icon(icon_name: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;

    let search_paths = [
        format!("{}/.local/share/flatpak/appstream", home),
        "/var/lib/flatpak/appstream".to_string(),
    ];

    for base_path in &search_paths {
        let Ok(repos) = std::fs::read_dir(base_path) else {
            continue;
        };

        for repo in repos.flatten().filter(|e| e.path().is_dir()) {
            let Ok(arches) = std::fs::read_dir(repo.path()) else {
                continue;
            };

            for arch in arches.flatten().filter(|e| e.path().is_dir()) {
                let Ok(hashes) = std::fs::read_dir(arch.path()) else {
                    continue;
                };

                for hash in hashes.flatten().filter(|e| e.path().is_dir()) {
                    if let Some(icon_path) = try_icon_sizes(&hash.path(), icon_name) {
                        return Some(icon_path);
                    }
                }
            }
        }
    }

    None
}

impl AppletIconData {
    fn new(applet: &Context) -> Self {
        let icon_size = applet.suggested_size(false).0;
        let (major_padding, cross_padding) = applet.suggested_padding(false);
        let (h_padding, v_padding) = if applet.is_horizontal() {
            (major_padding as f32, cross_padding as f32)
        } else {
            (cross_padding as f32, major_padding as f32)
        };
        let icon_spacing = applet.spacing as f32;

        let (dot_radius, bar_size) = match applet.size {
            Size::Hardcoded(_) => (2.0, 8.0),
            Size::PanelSize(ref s) => {
                let size = s.get_applet_icon_size_with_padding(false);
                // Define size thresholds, to handle custom sizes
                let small_size_threshold = PanelSize::S.get_applet_icon_size_with_padding(false);
                let medium_size_threshold = PanelSize::M.get_applet_icon_size_with_padding(false);
                if size <= small_size_threshold {
                    (1.0, 8.0)
                } else if size <= medium_size_threshold {
                    (2.0, 8.0)
                } else {
                    (2.0, 12.0)
                }
            }
        };
        let padding = match applet.anchor {
            PanelAnchor::Top => [
                v_padding - (dot_radius * 2. + 1.),
                h_padding,
                v_padding,
                h_padding,
            ],
            PanelAnchor::Bottom => [
                v_padding,
                h_padding,
                v_padding - (dot_radius * 2. + 1.),
                h_padding,
            ],
            PanelAnchor::Left => [
                v_padding,
                h_padding,
                v_padding,
                h_padding - (dot_radius * 2. + 1.),
            ],
            PanelAnchor::Right => [
                v_padding,
                h_padding - (dot_radius * 2. + 1.),
                v_padding,
                h_padding,
            ],
        };
        AppletIconData {
            icon_size,
            icon_spacing,
            dot_radius,
            bar_size,
            padding: padding.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockItemId {
    Item(u32),
    ActiveOverflow,
    FavoritesOverflow,
}

impl From<u32> for DockItemId {
    fn from(id: u32) -> Self {
        DockItemId::Item(id)
    }
}

impl From<usize> for DockItemId {
    fn from(id: usize) -> Self {
        DockItemId::Item(id as u32)
    }
}

#[derive(Debug, Clone)]
struct DockItem {
    // ID used internally in the applet. Each dock item
    // have an unique id
    id: u32,
    toplevels: Vec<(ToplevelInfo, Option<WaylandImage>)>,
    // Information found in the .desktop file
    desktop_info: DesktopEntry,
    // We must use this because the id in `DesktopEntry` is an estimation.
    // Thus, if we unpin an item, we want to be sure to use the real id
    original_app_id: String,
}

impl DockItem {
    fn as_icon(
        &self,
        applet: &Context,
        rectangle_tracker: Option<&RectangleTracker<DockItemId>>,
        interaction_enabled: bool,
        dnd_source_enabled: bool,
        gpus: Option<&[Gpu]>,
        is_focused: bool,
        dot_border_radius: [f32; 4],
        window_id: window::Id,
    ) -> Element<'_, Message> {
        let Self {
            toplevels,
            desktop_info,
            id,
            ..
        } = self;

        let app_icon = AppletIconData::new(applet);

        let icon_name = desktop_info.icon().unwrap_or_default();
        if icon_name.is_empty() {
            tracing::warn!("App (id: {}) has no icon specified in desktop file", id);
        } else {
            tracing::debug!("Loading icon '{}' for app id {}", icon_name, id);
        }

        let cosmic_icon = icon_source_with_flatpak_fallback(&icon_name)
            .as_cosmic_icon()
            // sets the preferred icon size variant
            .size(128)
            .width(app_icon.icon_size.into())
            .height(app_icon.icon_size.into());

        let indicator = {
            let container = if toplevels.len() <= 1 {
                vertical_space().height(Length::Fixed(0.0))
            } else {
                match applet.anchor {
                    PanelAnchor::Left | PanelAnchor::Right => {
                        vertical_space().height(app_icon.bar_size)
                    }
                    PanelAnchor::Top | PanelAnchor::Bottom => {
                        horizontal_space().width(app_icon.bar_size)
                    }
                }
            }
            .apply(container)
            .padding(app_icon.dot_radius);

            if toplevels.is_empty() {
                container
            } else {
                container.class(theme::Container::custom(move |theme| container::Style {
                    background: if is_focused {
                        Some(Background::Color(theme.cosmic().accent_color().into()))
                    } else {
                        Some(Background::Color(theme.cosmic().on_bg_color().into()))
                    },
                    border: Border {
                        radius: dot_border_radius.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }))
            }
        };

        let icon_wrapper: Element<_> = match applet.anchor {
            PanelAnchor::Left => row([
                indicator.into(),
                horizontal_space().width(Length::Fixed(1.0)).into(),
                cosmic_icon.clone().into(),
            ])
            .align_y(Alignment::Center)
            .into(),
            PanelAnchor::Right => row([
                cosmic_icon.clone().into(),
                horizontal_space().width(Length::Fixed(1.0)).into(),
                indicator.into(),
            ])
            .align_y(Alignment::Center)
            .into(),
            PanelAnchor::Top => column([
                indicator.into(),
                vertical_space().height(Length::Fixed(1.0)).into(),
                cosmic_icon.clone().into(),
            ])
            .align_x(Alignment::Center)
            .into(),
            PanelAnchor::Bottom => column([
                cosmic_icon.clone().into(),
                vertical_space().height(Length::Fixed(1.0)).into(),
                indicator.into(),
            ])
            .align_x(Alignment::Center)
            .into(),
        };

        let icon_button = button::custom(icon_wrapper)
            .padding(app_icon.padding)
            .selected(is_focused)
            .class(app_list_icon_style(is_focused));

        let icon_button: Element<_> = if interaction_enabled {
            mouse_area(
                icon_button
                    .on_press_maybe(if toplevels.is_empty() {
                        launch_on_preferred_gpu(desktop_info, gpus)
                    } else if toplevels.len() == 1 {
                        toplevels
                            .first()
                            .map(|t| Message::Toggle(t.0.foreign_toplevel.clone()))
                    } else {
                        Some(Message::TopLevelListPopup(*id, window_id))
                    })
                    .width(Length::Shrink)
                    .height(Length::Shrink),
            )
            .on_right_release(Message::Popup(*id, window_id))
            .on_middle_release({
                launch_on_preferred_gpu(desktop_info, gpus)
                    .unwrap_or(Message::Popup(*id, window_id))
            })
            .into()
        } else {
            icon_button.into()
        };

        let path = desktop_info.path.clone();
        let icon_button = if dnd_source_enabled && interaction_enabled {
            dnd_source(icon_button)
                .window(window_id)
                .drag_icon(move |_| {
                    (
                        cosmic_icon.clone().into(),
                        iced::core::widget::tree::State::None,
                        iced::Vector::ZERO,
                    )
                })
                .drag_threshold(16.)
                .drag_content(move || DndPathBuf(path.clone()))
                .on_start(Some(Message::StartDrag(*id)))
                .on_cancel(Some(Message::DragFinished))
                .on_finish(Some(Message::DragFinished))
        } else {
            dnd_source(icon_button)
        };

        if let Some(tracker) = rectangle_tracker {
            tracker.container((*id).into(), icon_button).into()
        } else {
            icon_button.into()
        }
    }
}

#[derive(Debug, Clone, Default)]
struct DndOffer {
    dock_item: Option<DockItem>,
    preview_index: usize,
}

#[derive(Debug, Clone)]
pub struct Popup {
    parent: window::Id,
    id: window::Id,
    dock_item: DockItem,
    popup_type: PopupType,
}

#[derive(Clone, Default)]
struct CosmicAppList {
    core: cosmic::app::Core,
    popup: Option<Popup>,
    subscription_ctr: u32,
    item_ctr: u32,
    desktop_entries: Vec<DesktopEntry>,
    active_list: Vec<DockItem>,
    pinned_list: Vec<DockItem>,
    dnd_source: Option<(window::Id, DockItem, DndAction, Option<usize>)>,
    config: AppListConfig,
    wayland_sender: Option<Sender<WaylandRequest>>,
    seat: Option<WlSeat>,
    rectangle_tracker: Option<RectangleTracker<DockItemId>>,
    rectangles: FxHashMap<DockItemId, iced::Rectangle>,
    dnd_offer: Option<DndOffer>,
    is_listening_for_dnd: bool,
    gpus: Option<Vec<Gpu>>,
    active_workspaces: Vec<ExtWorkspaceHandleV1>,
    output_list: FxHashMap<WlOutput, OutputInfo>,
    locales: Vec<String>,
    overflow_favorites_popup: Option<window::Id>,
    overflow_active_popup: Option<window::Id>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PopupType {
    RightClickMenu,
    TopLevelList,
}

#[derive(Debug, Clone)]
enum Message {
    Wayland(WaylandUpdate),
    PinApp(u32),
    UnpinApp(u32),
    Popup(u32, window::Id),
    Pressed(window::Id),
    TopLevelListPopup(u32, window::Id),
    GpuRequest(Option<Vec<Gpu>>),
    CloseRequested(window::Id),
    ClosePopup,
    Activate(ExtForeignToplevelHandleV1),
    Toggle(ExtForeignToplevelHandleV1),
    Exec(String, Option<usize>, bool),
    Quit(String),
    NewSeat(WlSeat),
    RemovedSeat,
    Rectangle(RectangleUpdate<DockItemId>),
    StartDrag(u32),
    DragFinished,
    DndEnter(f64, f64),
    DndLeave,
    DndMotion(f64, f64),
    DndDropFinished,
    DndData(Option<DndPathBuf>),
    StartListeningForDnd,
    StopListeningForDnd,
    IncrementSubscriptionCtr,
    ConfigUpdated(AppListConfig),
    OpenFavorites,
    OpenActive,
    Surface(surface::Action),
}

fn index_in_list(
    mut list_len: usize,
    item_size: f32,
    divider_size: f32,
    existing_preview: Option<usize>,
    pos_in_list: f32,
) -> usize {
    if existing_preview.is_some() {
        list_len += 1;
    }

    let index = if (list_len == 0) || (pos_in_list < item_size / 2.0) {
        0
    } else {
        let mut i = 1;
        let mut pos = item_size / 2.0;
        while i < list_len {
            let next_pos = pos + item_size + divider_size;
            if pos < pos_in_list && pos_in_list < next_pos {
                break;
            }
            pos = next_pos;
            i += 1;
        }
        i
    };

    if let Some(existing_preview) = existing_preview {
        if index >= existing_preview {
            index.saturating_sub(1)
        } else {
            index
        }
    } else {
        index
    }
}

async fn try_get_gpus() -> Option<Vec<Gpu>> {
    let connection = zbus::Connection::system().await.ok()?;
    let proxy = switcheroo_control::SwitcherooControlProxy::new(&connection)
        .await
        .ok()?;

    if !proxy.has_dual_gpu().await.ok()? {
        return None;
    }

    let gpus = proxy.get_gpus().await.ok()?;
    if gpus.is_empty() {
        return None;
    }

    Some(gpus)
}

const TOPLEVEL_BUTTON_WIDTH: f32 = 160.0;
const TOPLEVEL_BUTTON_HEIGHT: f32 = 130.0;

pub fn toplevel_button<'a, Msg>(
    img: Option<WaylandImage>,
    on_press: Msg,
    title: String,
    is_focused: bool,
) -> cosmic::widget::Button<'a, Msg>
where
    Msg: 'static + Clone,
{
    let border = 1.0;
    button::custom(
        container(
            column![
                container(if let Some(img) = img {
                    Element::from(Image::new(Handle::from_rgba(
                        img.width,
                        img.height,
                        img.img.clone(),
                    )))
                } else {
                    // Use a visible fallback icon instead of invisible 1x1 black pixel
                    icon::icon(from_name("application-x-executable").into())
                        .size(64)
                        .into()
                })
                .class(Container::Custom(Box::new(move |theme| {
                    container::Style {
                        border: Border {
                            color: theme.cosmic().bg_divider().into(),
                            width: border,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    }
                })))
                .padding(border as u16)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .apply(container)
                .center_y(Length::Fixed(90.0)),
                text::body(title),
            ]
            .spacing(4)
            .align_x(Alignment::Center),
        )
        .center(Length::Fill),
    )
    .on_press(on_press)
    .class(window_menu_style(is_focused))
    .width(Length::Fixed(TOPLEVEL_BUTTON_WIDTH))
    .height(Length::Fixed(TOPLEVEL_BUTTON_HEIGHT))
    .selected(is_focused)
}

fn window_menu_style(selected: bool) -> cosmic::theme::Button {
    let radius = theme::active()
        .cosmic()
        .radius_m()
        .map(|x| if x < 8.0 { x } else { x - 4.0 });

    Button::Custom {
        active: Box::new(move |focused, theme| {
            let a = button::Catalog::active(theme, focused, selected, &Button::AppletMenu);
            button::Style {
                background: if selected {
                    Some(Background::Color(
                        theme.cosmic().icon_button.selected_state_color().into(),
                    ))
                } else {
                    a.background
                },
                border_radius: radius.into(),
                outline_width: 0.0,
                ..a
            }
        }),
        hovered: Box::new(move |focused, theme| {
            let focused = selected || focused;
            let text = button::Catalog::hovered(theme, focused, focused, &Button::AppletMenu);
            button::Style {
                border_radius: radius.into(),
                outline_width: 0.0,
                ..text
            }
        }),
        disabled: Box::new(move |theme| {
            let text = button::Catalog::disabled(theme, &Button::AppletMenu);
            button::Style {
                border_radius: radius.into(),
                outline_width: 0.0,
                ..text
            }
        }),
        pressed: Box::new(move |focused, theme| {
            let focused = selected || focused;
            let text = button::Catalog::pressed(theme, focused, focused, &Button::AppletMenu);
            button::Style {
                border_radius: radius.into(),
                outline_width: 0.0,
                ..text
            }
        }),
    }
}

fn app_list_icon_style(selected: bool) -> cosmic::theme::Button {
    Button::Custom {
        active: Box::new(move |focused, theme| {
            let a = button::Catalog::active(theme, focused, selected, &Button::AppletIcon);
            button::Style {
                background: if selected {
                    Some(Background::Color(
                        theme.cosmic().icon_button.selected_state_color().into(),
                    ))
                } else {
                    a.background
                },
                ..a
            }
        }),
        hovered: Box::new(move |focused, theme| {
            button::Catalog::hovered(theme, focused, selected, &Button::AppletIcon)
        }),
        disabled: Box::new(|theme| button::Catalog::disabled(theme, &Button::AppletIcon)),
        pressed: Box::new(move |focused, theme| {
            button::Catalog::pressed(theme, focused, selected, &Button::AppletIcon)
        }),
    }
}

#[inline]
pub fn menu_control_padding() -> Padding {
    let spacing = cosmic::theme::spacing();
    [spacing.space_xxs, spacing.space_s].into()
}

fn find_desktop_entries<'a>(
    desktop_entries: &'a [fde::DesktopEntry],
    app_ids: &'a [String],
) -> impl Iterator<Item = fde::DesktopEntry> + 'a {
    app_ids.iter().map(|fav| {
        let unicase_fav = fde::unicase::Ascii::new(fav.as_str());
        fde::find_app_by_id(desktop_entries, unicase_fav).map_or_else(
            || fde::DesktopEntry::from_appid(fav.clone()),
            ToOwned::to_owned,
        )
    })
}

impl CosmicAppList {
    // Cache all desktop entries to use when new apps are added to the dock.
    fn update_desktop_entries(&mut self) {
        self.desktop_entries = fde::Iter::new(fde::default_paths())
            .filter_map(|p| fde::DesktopEntry::from_path(p, Some(&self.locales)).ok())
            .collect::<Vec<_>>();
    }

    // Update pinned items using the cached desktop entries as a source.
    fn update_pinned_list(&mut self) {
        self.pinned_list = find_desktop_entries(&self.desktop_entries, &self.config.favorites)
            .zip(&self.config.favorites)
            .enumerate()
            .map(|(pinned_ctr, (e, original_id))| DockItem {
                id: pinned_ctr as u32,
                toplevels: Vec::new(),
                desktop_info: e,
                original_app_id: original_id.clone(),
            })
            .collect();
    }
}

impl cosmic::Application for CosmicAppList {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = APP_ID;

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        let config = Config::new(APP_ID, AppListConfig::VERSION)
            .ok()
            .and_then(|c| AppListConfig::get_entry(&c).ok())
            .unwrap_or_default();

        let mut app_list = Self {
            core,
            config,
            locales: get_languages_from_env(),
            ..Default::default()
        };

        app_list.update_desktop_entries();
        app_list.update_pinned_list();

        app_list.item_ctr = app_list.pinned_list.len() as u32;

        (
            app_list,
            Task::perform(try_get_gpus(), |gpus| {
                cosmic::Action::App(Message::GpuRequest(gpus))
            }),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::Popup(id, parent_window_id) => {
                if let Some(Popup {
                    parent,
                    id: popup_id,
                    ..
                }) = self.popup.take()
                {
                    if parent == parent_window_id {
                        return destroy_popup(popup_id);
                    } else {
                        self.overflow_active_popup = None;
                        self.overflow_favorites_popup = None;
                        return Task::batch([destroy_popup(popup_id), destroy_popup(parent)]);
                    }
                }
                if let Some(toplevel_group) = self
                    .active_list
                    .iter()
                    .chain(self.pinned_list.iter())
                    .find(|t| t.id == id)
                {
                    let Some(rectangle) = self.rectangles.get(&toplevel_group.id.into()) else {
                        tracing::error!("No rectangle found for toplevel group");
                        return Task::none();
                    };

                    let new_id = window::Id::unique();
                    self.popup = Some(Popup {
                        parent: parent_window_id,
                        id: new_id,
                        dock_item: toplevel_group.clone(),
                        popup_type: PopupType::RightClickMenu,
                    });

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        parent_window_id,
                        new_id,
                        None,
                        None,
                        None,
                    );
                    let iced::Rectangle {
                        x,
                        y,
                        width,
                        height,
                    } = *rectangle;
                    popup_settings.positioner.anchor_rect = iced::Rectangle::<i32> {
                        x: x as i32,
                        y: y as i32,
                        width: width as i32,
                        height: height as i32,
                    };

                    let gpu_update = Task::perform(try_get_gpus(), |gpus| {
                        cosmic::Action::App(Message::GpuRequest(gpus))
                    });
                    return Task::batch([gpu_update, get_popup(popup_settings)]);
                }
            }
            Message::TopLevelListPopup(id, parent_window_id) => {
                if let Some(Popup {
                    parent,
                    id: popup_id,
                    ..
                }) = self.popup.take()
                {
                    if parent == parent_window_id {
                        return destroy_popup(popup_id);
                    } else {
                        self.overflow_active_popup = None;
                        self.overflow_favorites_popup = None;
                        return Task::batch([destroy_popup(popup_id), destroy_popup(parent)]);
                    }
                }
                if let Some(toplevel_group) = self
                    .active_list
                    .iter()
                    .chain(self.pinned_list.iter())
                    .find(|t| t.id == id)
                {
                    for (info, _) in &toplevel_group.toplevels {
                        if let Some(tx) = self.wayland_sender.as_ref() {
                            let _ =
                                tx.send(WaylandRequest::Screencopy(info.foreign_toplevel.clone()));
                        }
                    }

                    let Some(rectangle) = self.rectangles.get(&toplevel_group.id.into()) else {
                        return Task::none();
                    };

                    let new_id = window::Id::unique();
                    self.popup = Some(Popup {
                        parent: parent_window_id,
                        id: new_id,
                        dock_item: toplevel_group.clone(),
                        popup_type: PopupType::TopLevelList,
                    });

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        parent_window_id,
                        new_id,
                        None,
                        None,
                        None,
                    );
                    let iced::Rectangle {
                        x,
                        y,
                        width,
                        height,
                    } = *rectangle;
                    popup_settings.positioner.anchor_rect = iced::Rectangle::<i32> {
                        x: x as i32,
                        y: y as i32,
                        width: width as i32,
                        height: height as i32,
                    };
                    let max_windows = 7.0;
                    let window_spacing = 8.0;
                    popup_settings.positioner.size_limits = match self.core.applet.anchor {
                        PanelAnchor::Right | PanelAnchor::Left => Limits::NONE
                            .min_width(100.0)
                            .min_height(30.0)
                            .max_width(window_spacing * 2.0 + TOPLEVEL_BUTTON_WIDTH)
                            .max_height(
                                TOPLEVEL_BUTTON_HEIGHT * max_windows
                                    + window_spacing * (max_windows + 1.0),
                            ),
                        PanelAnchor::Bottom | PanelAnchor::Top => Limits::NONE
                            .min_width(30.0)
                            .min_height(100.0)
                            .max_width(
                                TOPLEVEL_BUTTON_WIDTH * max_windows
                                    + window_spacing * (max_windows + 1.0),
                            )
                            .max_height(window_spacing * 2.0 + TOPLEVEL_BUTTON_HEIGHT),
                    };

                    return get_popup(popup_settings);
                }
            }
            Message::PinApp(id) => {
                if let Some(i) = self.active_list.iter().position(|t| t.id == id) {
                    let entry = self.active_list.remove(i);
                    self.config.add_pinned(
                        entry.original_app_id.clone(),
                        &Config::new(APP_ID, AppListConfig::VERSION).unwrap(),
                    );
                    self.pinned_list.push(entry);
                }
                if let Some(Popup { id: popup_id, .. }) = self.popup.take() {
                    return destroy_popup(popup_id);
                }
            }
            Message::UnpinApp(id) => {
                if let Some(i) = self.pinned_list.iter().position(|t| t.id == id) {
                    let entry = self.pinned_list.remove(i);

                    self.config.remove_pinned(
                        &entry.original_app_id,
                        &Config::new(APP_ID, AppListConfig::VERSION).unwrap(),
                    );

                    self.rectangles.remove(&entry.id.into());
                    if !entry.toplevels.is_empty() {
                        self.active_list.push(entry);
                    }
                }
                if let Some(Popup { id: popup_id, .. }) = self.popup.take() {
                    return destroy_popup(popup_id);
                }
            }
            Message::Activate(handle) => {
                if let Some(tx) = self.wayland_sender.as_ref() {
                    let _ = tx.send(WaylandRequest::Toplevel(ToplevelRequest::Activate(handle)));
                }
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p.id);
                }
            }
            Message::Toggle(handle) => {
                if let Some(tx) = self.wayland_sender.as_ref() {
                    let _ = tx.send(WaylandRequest::Toplevel(
                        if self.currently_active_toplevel().contains(&handle) {
                            ToplevelRequest::Minimize(handle)
                        } else {
                            ToplevelRequest::Activate(handle)
                        },
                    ));
                }
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p.id);
                }
            }
            Message::Quit(id) => {
                if let Some(toplevel_group) = self
                    .active_list
                    .iter()
                    .chain(self.pinned_list.iter())
                    .find(|t| t.desktop_info.id() == id)
                {
                    for (info, _) in &toplevel_group.toplevels {
                        if let Some(tx) = self.wayland_sender.as_ref() {
                            let _ = tx.send(WaylandRequest::Toplevel(ToplevelRequest::Quit(
                                info.foreign_toplevel.clone(),
                            )));
                        }
                    }
                }
                if let Some(Popup { id: popup_id, .. }) = self.popup.take() {
                    return destroy_popup(popup_id);
                }
            }
            Message::StartDrag(id) => {
                if let Some((_, toplevel_group, pos)) = self
                    .active_list
                    .iter()
                    .find_map(|t| {
                        if t.id == id {
                            Some((false, t.clone(), None))
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        self.pinned_list
                            .iter()
                            .position(|t| t.id == id)
                            .map(|pos| (true, self.pinned_list[pos].clone(), Some(pos)))
                    })
                {
                    let icon_id = window::Id::unique();
                    self.dnd_source =
                        Some((icon_id, toplevel_group.clone(), DndAction::empty(), pos));
                }
            }
            Message::DragFinished => {
                if let Some((_, mut toplevel_group, _, _pinned_pos)) = self.dnd_source.take() {
                    if self.dnd_offer.take().is_some() {
                        if let Some((_, toplevel_group, _, pinned_pos)) = self.dnd_source.as_ref() {
                            let mut pos = 0;
                            self.pinned_list.retain_mut(|pinned| {
                                let matched_id =
                                    pinned.desktop_info.id() == toplevel_group.desktop_info.id();
                                let pinned_match =
                                    pinned_pos.is_some_and(|pinned_pos| pinned_pos == pos);
                                let ret = !matched_id || pinned_match;

                                pos += 1;
                                ret
                            });
                        }
                    }

                    if !self
                        .pinned_list
                        .iter()
                        .chain(self.active_list.iter())
                        .any(|t| t.desktop_info.id() == toplevel_group.desktop_info.id())
                        && !toplevel_group.toplevels.is_empty()
                    {
                        self.item_ctr += 1;
                        toplevel_group.id = self.item_ctr;
                        self.active_list.push(toplevel_group);
                    }
                }
            }
            Message::DndEnter(x, y) => {
                let item_size = self.core.applet.suggested_size(false).0
                    + 2 * self.core.applet.suggested_padding(false).0;
                let pos_in_list = match self.core.applet.anchor {
                    PanelAnchor::Top | PanelAnchor::Bottom => x as f32,
                    PanelAnchor::Left | PanelAnchor::Right => y as f32,
                };
                let num_pinned = self.pinned_list.len();
                let index = index_in_list(num_pinned, item_size as f32, 4.0, None, pos_in_list);
                self.dnd_offer = Some(DndOffer {
                    preview_index: index,
                    ..DndOffer::default()
                });
                if let Some(dnd_source) = self.dnd_source.as_ref() {
                    self.dnd_offer.as_mut().unwrap().dock_item = Some(dnd_source.1.clone());
                } else {
                    // TODO dnd
                    return peek_dnd::<DndPathBuf>()
                        .map(Message::DndData)
                        .map(cosmic::Action::App);
                }
            }
            Message::DndMotion(x, y) => {
                let item_size = self.core.applet.suggested_size(false).0
                    + 2 * self.core.applet.suggested_padding(false).0;
                let pos_in_list = match self.core.applet.anchor {
                    PanelAnchor::Top | PanelAnchor::Bottom => x as f32,
                    PanelAnchor::Left | PanelAnchor::Right => y as f32,
                };
                let num_pinned = self.pinned_list.len();
                let index = index_in_list(
                    num_pinned,
                    item_size as f32,
                    4.0,
                    self.dnd_offer.as_ref().map(|o| o.preview_index),
                    pos_in_list,
                );
                if let Some(o) = self.dnd_offer.as_mut() {
                    o.preview_index = index;
                }
            }
            Message::DndLeave => {
                if let Some((_, toplevel_group, _, pinned_pos)) = self.dnd_source.as_ref() {
                    let mut pos = 0;
                    self.pinned_list.retain_mut(|pinned| {
                        let matched_id =
                            pinned.desktop_info.id() == toplevel_group.desktop_info.id();
                        let pinned_match = pinned_pos.is_some_and(|pinned_pos| pinned_pos == pos);
                        let ret = !matched_id || pinned_match;

                        pos += 1;
                        ret
                    });
                }
                self.dnd_offer = None;
            }
            Message::DndData(file_path) => {
                let Some(file_path) = file_path else {
                    tracing::error!("Couldn't peek at hovered path.");
                    return Task::none();
                };
                if let Some(DndOffer { dock_item, .. }) = self.dnd_offer.as_mut() {
                    if let Ok(de) = fde::DesktopEntry::from_path(file_path.0, Some(&self.locales)) {
                        self.item_ctr += 1;
                        *dock_item = Some(DockItem {
                            id: self.item_ctr,
                            toplevels: Vec::new(),
                            original_app_id: de.id().to_string(),
                            desktop_info: de,
                        });
                    }
                }
            }
            Message::DndDropFinished => {
                // we actually should have the data already, if not, we probably shouldn't do
                // anything anyway
                if let Some((mut dock_item, index)) = self
                    .dnd_offer
                    .take()
                    .and_then(|o| o.dock_item.map(|i| (i, o.preview_index)))
                {
                    self.item_ctr += 1;

                    if let Some((pos, is_pinned)) = self
                        .active_list
                        .iter()
                        .position(|de| de.original_app_id == dock_item.original_app_id)
                        .map(|pos| (pos, false))
                        .or_else(|| {
                            self.pinned_list
                                .iter()
                                .position(|de| de.original_app_id == dock_item.original_app_id)
                                .map(|pos| (pos, true))
                        })
                    {
                        let t = if is_pinned {
                            let t = self.pinned_list.remove(pos);
                            self.config.remove_pinned(
                                &t.original_app_id,
                                &Config::new(APP_ID, AppListConfig::VERSION).unwrap(),
                            );
                            t
                        } else {
                            self.active_list.remove(pos)
                        };
                        dock_item.toplevels = t.toplevels;
                    }
                    dock_item.id = self.item_ctr;

                    if dock_item.desktop_info.exec().is_some() {
                        self.pinned_list
                            .insert(index.min(self.pinned_list.len()), dock_item);
                        self.config.update_pinned(
                            self.pinned_list
                                .iter()
                                .map(|dock_item| dock_item.original_app_id.clone())
                                .collect(),
                            &Config::new(APP_ID, AppListConfig::VERSION).unwrap(),
                        );
                    }
                }
            }
            Message::Wayland(event) => {
                match event {
                    WaylandUpdate::Init(tx) => {
                        self.wayland_sender.replace(tx);
                    }
                    WaylandUpdate::Image(handle, img) => {
                        'img_update: for x in self
                            .active_list
                            .iter_mut()
                            .chain(self.pinned_list.iter_mut())
                        {
                            if let Some((_, handle_img)) = x
                                .toplevels
                                .iter_mut()
                                .find(|(info, _)| info.foreign_toplevel == handle)
                            {
                                *handle_img = Some(img);
                                break 'img_update;
                            }
                        }
                    }
                    WaylandUpdate::Finished => {
                        for t in &mut self.pinned_list {
                            t.toplevels.clear();
                        }
                        self.active_list.clear();
                        let subscription_ctr = self.subscription_ctr;
                        let rand_d = fastrand::u64(0..100);
                        return iced::Task::perform(
                            async move {
                                if let Some(millis) = 2u64
                                    .checked_pow(subscription_ctr)
                                    .and_then(|d| d.checked_add(rand_d))
                                {
                                    sleep(Duration::from_millis(millis)).await;
                                } else {
                                    pending::<()>().await;
                                }
                            },
                            |()| Message::IncrementSubscriptionCtr,
                        )
                        .map(cosmic::action::app);
                    }
                    WaylandUpdate::Toplevel(event) => match event {
                        ToplevelUpdate::Add(mut info) => {
                            let unicase_appid = fde::unicase::Ascii::new(&*info.app_id);
                            let new_desktop_info =
                                self.find_desktop_entry_for_toplevel(&info, unicase_appid);

                            if let Some(t) = self
                                .active_list
                                .iter_mut()
                                .chain(self.pinned_list.iter_mut())
                                .find(|DockItem { desktop_info, .. }| {
                                    desktop_info.id() == new_desktop_info.id()
                                })
                            {
                                t.toplevels.push((info, None));
                            } else {
                                if info.app_id.is_empty() {
                                    info.app_id = format!("Unknown Application {}", self.item_ctr);
                                }
                                self.item_ctr += 1;

                                self.active_list.push(DockItem {
                                    id: self.item_ctr,
                                    original_app_id: info.app_id.clone(),
                                    toplevels: vec![(info, None)],
                                    desktop_info: new_desktop_info,
                                });
                            }
                        }
                        ToplevelUpdate::Remove(handle) => {
                            for t in self
                                .active_list
                                .iter_mut()
                                .chain(self.pinned_list.iter_mut())
                            {
                                t.toplevels
                                    .retain(|(info, _)| info.foreign_toplevel != handle);
                            }
                            self.active_list.retain(|t| !t.toplevels.is_empty());
                        }
                        ToplevelUpdate::Update(info) => {
                            // TODO probably want to make sure it is removed
                            if info.app_id.is_empty() {
                                return Task::none();
                            }
                            let mut updated_appid = false;

                            'toplevel_loop: for toplevel_list in self
                                .active_list
                                .iter_mut()
                                .chain(self.pinned_list.iter_mut())
                            {
                                for (t_info, _) in &mut toplevel_list.toplevels {
                                    if info.foreign_toplevel == t_info.foreign_toplevel {
                                        if info.app_id != t_info.app_id {
                                            updated_appid = true;
                                        }

                                        *t_info = info.clone();
                                        break 'toplevel_loop;
                                    }
                                }
                            }

                            if updated_appid {
                                // remove the current toplevel from its dock item
                                for t in self
                                    .active_list
                                    .iter_mut()
                                    .chain(self.pinned_list.iter_mut())
                                {
                                    t.toplevels
                                        .retain(|(t_info, _)| t_info.app_id != info.app_id);
                                }
                                self.active_list.retain(|t| !t.toplevels.is_empty());

                                // find a new one for it
                                let new_desktop_entry = self.find_desktop_entry_for_toplevel(
                                    &info,
                                    Ascii::new(&info.app_id),
                                );

                                if let Some(t) = self
                                    .active_list
                                    .iter_mut()
                                    .chain(self.pinned_list.iter_mut())
                                    .find(|DockItem { desktop_info, .. }| {
                                        desktop_info.id() == new_desktop_entry.id()
                                    })
                                {
                                    t.toplevels.push((info, None));
                                } else {
                                    self.item_ctr += 1;

                                    self.active_list.push(DockItem {
                                        id: self.item_ctr,
                                        original_app_id: info.app_id.clone(),
                                        toplevels: vec![(info, None)],
                                        desktop_info: new_desktop_entry,
                                    });
                                }
                            }
                        }
                    },
                    WaylandUpdate::Workspace(workspaces) => self.active_workspaces = workspaces,
                    WaylandUpdate::Output(event) => match event {
                        OutputUpdate::Add(output, info) => {
                            self.output_list.insert(output, info);
                        }
                        OutputUpdate::Update(output, info) => {
                            self.output_list.insert(output, info);
                        }
                        OutputUpdate::Remove(output) => {
                            self.output_list.remove(&output);
                        }
                    },
                    WaylandUpdate::ActivationToken {
                        token,
                        app_id,
                        exec,
                        gpu_idx,
                        terminal,
                    } => {
                        let mut envs = Vec::new();
                        if let Some(token) = token {
                            envs.push(("XDG_ACTIVATION_TOKEN".to_string(), token.clone()));
                            envs.push(("DESKTOP_STARTUP_ID".to_string(), token));
                        }
                        if let (Some(gpus), Some(idx)) = (self.gpus.as_ref(), gpu_idx) {
                            envs.extend(
                                gpus[idx]
                                    .environment
                                    .iter()
                                    .map(|(k, v)| (k.clone(), v.clone())),
                            );
                        }
                        tokio::spawn(async move {
                            cosmic::desktop::spawn_desktop_exec(
                                exec,
                                envs,
                                app_id.as_deref(),
                                terminal,
                            )
                            .await;
                        });
                    }
                }
            }
            Message::NewSeat(s) => {
                self.seat.replace(s);
            }
            Message::RemovedSeat => {
                self.seat.take();
            }
            Message::Exec(exec, gpu_idx, terminal) => {
                if let Some(tx) = self.wayland_sender.as_ref() {
                    let _ = tx.send(WaylandRequest::TokenRequest {
                        app_id: Self::APP_ID.to_string(),
                        exec,
                        gpu_idx,
                        terminal,
                    });
                }
            }
            Message::Rectangle(u) => match u {
                RectangleUpdate::Rectangle(r) => {
                    self.rectangles.insert(r.0, r.1);
                }
                RectangleUpdate::Init(tracker) => {
                    self.rectangle_tracker.replace(tracker);
                }
            },
            Message::ClosePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p.id);
                }
            }
            Message::StartListeningForDnd => {
                self.is_listening_for_dnd = true;
            }
            Message::StopListeningForDnd => {
                self.is_listening_for_dnd = false;
            }
            Message::IncrementSubscriptionCtr => {
                self.subscription_ctr += 1;
            }
            Message::ConfigUpdated(config) => {
                self.config = config;
                // drain to active list
                for item in self.pinned_list.drain(..) {
                    if !item.toplevels.is_empty() {
                        self.active_list.push(item);
                    }
                }

                // pull back configured items into the favorites list
                self.pinned_list =
                    find_desktop_entries(&self.desktop_entries, &self.config.favorites)
                        .zip(&self.config.favorites)
                        .map(|(de, original_id)| {
                            if let Some(p) = self
                                .active_list
                                .iter()
                                // match using heuristic id
                                .position(|dock_item| dock_item.desktop_info.id() == de.id())
                            {
                                let mut d = self.active_list.remove(p);
                                // but use the id from the config
                                d.original_app_id.clone_from(original_id);
                                d
                            } else {
                                self.item_ctr += 1;
                                DockItem {
                                    id: self.item_ctr,
                                    toplevels: Vec::new(),
                                    desktop_info: de.clone(),
                                    original_app_id: original_id.clone(),
                                }
                            }
                        })
                        .collect();
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup.as_ref().map(|p| p.id) {
                    self.popup = None;
                }
                if self.overflow_active_popup.is_some_and(|p| p == id) {
                    self.overflow_active_popup = None;
                }
                if self.overflow_favorites_popup.is_some_and(|p| p == id) {
                    self.overflow_favorites_popup = None;
                }
            }
            Message::GpuRequest(gpus) => {
                self.gpus = gpus;
            }
            Message::OpenActive => {
                let create_new = self.overflow_active_popup.is_none();
                let mut cmds = vec![self.close_popups()];

                // create a popup with the active list
                if create_new {
                    let new_id = window::Id::unique();
                    self.overflow_active_popup = Some(new_id);
                    let rectangle = self.rectangles.get(&DockItemId::ActiveOverflow);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    if let Some(iced::Rectangle {
                        x,
                        y,
                        width,
                        height,
                    }) = rectangle
                    {
                        popup_settings.positioner.anchor_rect = iced::Rectangle::<i32> {
                            x: *x as i32,
                            y: *y as i32,
                            width: *width as i32,
                            height: *height as i32,
                        };
                    }
                    let applet_suggested_size = self.core.applet.suggested_size(false).0
                        + 2 * self.core.applet.suggested_padding(false).0;
                    let (_favorite_popup_cutoff, active_popup_cutoff) =
                        self.panel_overflow_lengths();
                    let popup_applet_count =
                        self.active_list.len().saturating_sub(
                            (active_popup_cutoff.unwrap_or_default()).saturating_sub(1),
                        ) as f32;
                    let popup_applet_size = applet_suggested_size as f32 * popup_applet_count
                        + 4.0 * (popup_applet_count - 1.);
                    let (max_width, max_height) = match self.core.applet.anchor {
                        PanelAnchor::Top | PanelAnchor::Bottom => {
                            (popup_applet_size, applet_suggested_size as f32)
                        }
                        PanelAnchor::Left | PanelAnchor::Right => {
                            (applet_suggested_size as f32, popup_applet_size)
                        }
                    };
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(max_width)
                        .min_width(1.)
                        .max_height(max_height)
                        .min_height(1.);
                    cmds.push(get_popup(popup_settings));
                }
                return Task::batch(cmds);
            }
            Message::OpenFavorites => {
                let create_new = self.overflow_favorites_popup.is_none();
                let mut cmds = vec![self.close_popups()];

                // create a popup with the favorites list
                if create_new {
                    let new_id = window::Id::unique();
                    self.overflow_favorites_popup = Some(new_id);
                    let rectangle = self.rectangles.get(&DockItemId::FavoritesOverflow);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    if let Some(iced::Rectangle {
                        x,
                        y,
                        width,
                        height,
                    }) = rectangle
                    {
                        popup_settings.positioner.anchor_rect = iced::Rectangle::<i32> {
                            x: *x as i32,
                            y: *y as i32,
                            width: *width as i32,
                            height: *height as i32,
                        };
                    }
                    let applet_suggested_size = self.core.applet.suggested_size(false).0
                        + 2 * self.core.applet.suggested_padding(false).0;
                    let (favorite_popup_cutoff, _active_popup_cutoff) =
                        self.panel_overflow_lengths();
                    let popup_applet_count =
                        self.pinned_list.len().saturating_sub(
                            favorite_popup_cutoff.unwrap_or_default().saturating_sub(1),
                        ) as f32;
                    let popup_applet_size = applet_suggested_size as f32 * popup_applet_count
                        + 4.0 * (popup_applet_count - 1.);
                    let (max_width, max_height) = match self.core.applet.anchor {
                        PanelAnchor::Top | PanelAnchor::Bottom => {
                            (popup_applet_size, applet_suggested_size as f32)
                        }
                        PanelAnchor::Left | PanelAnchor::Right => {
                            (applet_suggested_size as f32, popup_applet_size)
                        }
                    };
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(max_width)
                        .min_width(1.)
                        .max_height(max_height)
                        .min_height(1.);
                    cmds.push(get_popup(popup_settings));
                }
                return Task::batch(cmds);
            }
            Message::Pressed(id) => {
                if self.popup.is_some() && self.core.main_window_id() == Some(id) {
                    return self.close_popups();
                }
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let focused_item = self.currently_active_toplevel();
        let theme = self.core.system_theme();
        let dot_radius = theme.cosmic().radius_xs();
        let app_icon = AppletIconData::new(&self.core.applet);
        let is_horizontal = match self.core.applet.anchor {
            PanelAnchor::Top | PanelAnchor::Bottom => true,
            PanelAnchor::Left | PanelAnchor::Right => false,
        };
        let divider_padding = match self.core.applet.size {
            Size::Hardcoded(_) => 4,
            Size::PanelSize(ref s) => {
                let size = s.get_applet_icon_size_with_padding(false);

                let small_size_threshold = PanelSize::S.get_applet_icon_size_with_padding(false);

                if size <= small_size_threshold { 4 } else { 8 }
            }
        };
        let (favorite_popup_cutoff, active_popup_cutoff) = self.panel_overflow_lengths();
        let mut favorite_to_remove = if let Some(cutoff) = favorite_popup_cutoff {
            if cutoff < self.pinned_list.len() {
                self.pinned_list.len() - cutoff + 1
            } else {
                0
            }
        } else {
            0
        };
        let favorites: Vec<_> = self
            .pinned_list
            .iter()
            .rev()
            .filter(|f| {
                if favorite_to_remove > 0 && f.toplevels.is_empty() {
                    favorite_to_remove -= 1;
                    false
                } else {
                    true
                }
            })
            .collect();
        let mut favorites: Vec<_> = favorites[favorite_to_remove..]
            .iter()
            .rev()
            .map(|dock_item| {
                self.core
                    .applet
                    .applet_tooltip::<Message>(
                        dock_item.as_icon(
                            &self.core.applet,
                            self.rectangle_tracker.as_ref(),
                            self.popup.is_none(),
                            self.config.enable_drag_source,
                            self.gpus.as_deref(),
                            dock_item
                                .toplevels
                                .iter()
                                .any(|y| focused_item.contains(&y.0.foreign_toplevel)),
                            dot_radius,
                            self.core.main_window_id().unwrap(),
                        ),
                        dock_item
                            .desktop_info
                            .full_name(&self.locales)
                            .unwrap_or_default()
                            .into_owned(),
                        self.popup.is_some(),
                        Message::Surface,
                        None,
                    )
                    .into()
            })
            .collect();

        if favorite_popup_cutoff.is_some() {
            // button to show more favorites
            let icon = match self.core.applet.anchor {
                PanelAnchor::Bottom => "go-up-symbolic",
                PanelAnchor::Left => "go-next-symbolic",
                PanelAnchor::Right => "go-previous-symbolic",
                PanelAnchor::Top => "go-down-symbolic",
            };
            let btn = self
                .core
                .applet
                .icon_button(icon)
                .on_press(Message::OpenFavorites);
            let btn: Element<_> = if let Some(rectangle_tracker) = self.rectangle_tracker.as_ref() {
                rectangle_tracker
                    .container(DockItemId::FavoritesOverflow, btn)
                    .into()
            } else {
                btn.into()
            };
            favorites.push(btn);
        }

        if let Some((item, index)) = self
            .dnd_offer
            .as_ref()
            .and_then(|o| o.dock_item.as_ref().map(|item| (item, o.preview_index)))
        {
            favorites.insert(
                index.min(favorites.len()),
                item.as_icon(
                    &self.core.applet,
                    None,
                    false,
                    self.config.enable_drag_source,
                    self.gpus.as_deref(),
                    item.toplevels
                        .iter()
                        .any(|y| focused_item.contains(&y.0.foreign_toplevel)),
                    dot_radius,
                    self.core.main_window_id().unwrap(),
                ),
            );
        } else if self.is_listening_for_dnd && self.pinned_list.is_empty() {
            // show star indicating pinned_list is drag target
            favorites.push(
                container(
                    icon::from_name("starred-symbolic.symbolic")
                        .size(self.core.applet.suggested_size(false).0),
                )
                .padding(self.core.applet.suggested_padding(false).1) // TODO
                .into(),
            );
        }

        let mut active: Vec<_> =
            self.active_list[..active_popup_cutoff.map_or(self.active_list.len(), |n| {
                if n < self.active_list.len() {
                    n.saturating_sub(1)
                } else {
                    n
                }
            })]
                .iter()
                .map(|dock_item| {
                    self.core
                        .applet
                        .applet_tooltip(
                            dock_item.as_icon(
                                &self.core.applet,
                                self.rectangle_tracker.as_ref(),
                                self.popup.is_none(),
                                self.config.enable_drag_source,
                                self.gpus.as_deref(),
                                dock_item
                                    .toplevels
                                    .iter()
                                    .any(|y| focused_item.contains(&y.0.foreign_toplevel)),
                                dot_radius,
                                self.core.main_window_id().unwrap(),
                            ),
                            dock_item
                                .desktop_info
                                .full_name(&self.locales)
                                .unwrap_or_default()
                                .into_owned(),
                            self.popup.is_some(),
                            Message::Surface,
                            None,
                        )
                        .into()
                })
                .collect();

        if active_popup_cutoff.is_some_and(|n| n < self.active_list.len()) {
            // button to show more active
            let icon = match self.core.applet.anchor {
                PanelAnchor::Bottom => "go-up-symbolic",
                PanelAnchor::Left => "go-next-symbolic",
                PanelAnchor::Right => "go-previous-symbolic",
                PanelAnchor::Top => "go-down-symbolic",
            };
            let btn = self
                .core
                .applet
                .icon_button(icon)
                .on_press(Message::OpenActive);
            let btn: Element<_> = if let Some(rectangle_tracker) = self.rectangle_tracker.as_ref() {
                rectangle_tracker
                    .container(DockItemId::ActiveOverflow, btn)
                    .into()
            } else {
                btn.into()
            };
            active.push(btn);
        }

        let window_size = self.core.applet.suggested_bounds.as_ref();
        let max_num = if self.core.applet.is_horizontal() {
            let suggested_width = self.core.applet.suggested_size(false).0
                + self.core.applet.suggested_padding(false).0 * 2;
            window_size
                .map(|w| w.width)
                .map_or(u32::MAX, |b| (b / suggested_width as f32) as u32) as usize
        } else {
            let suggested_height = self.core.applet.suggested_size(false).1
                + self.core.applet.suggested_padding(false).0 * 2;
            window_size
                .map(|w| w.height)
                .map_or(u32::MAX, |b| (b / suggested_height as f32) as u32) as usize
        }
        .max(4);
        if max_num < favorites.len() + active.len() {
            let active_leftover = max_num.saturating_sub(favorites.len());
            favorites.truncate(max_num - active_leftover);
            active.truncate(active_leftover);
        }
        let (w, h, favorites, active, divider) = if is_horizontal {
            (
                Length::Shrink,
                Length::Shrink,
                DndDestination::for_data::<DndPathBuf>(
                    row(favorites).spacing(app_icon.icon_spacing),
                    |_, _| Message::DndDropFinished,
                )
                .drag_id(DND_FAVORITES),
                row(active).spacing(app_icon.icon_spacing).into(),
                container(vertical_rule(1))
                    .height(Length::Fill)
                    .padding([divider_padding, 0])
                    .into(),
            )
        } else {
            (
                Length::Shrink,
                Length::Shrink,
                DndDestination::for_data(
                    column(favorites).spacing(app_icon.icon_spacing),
                    |_data: Option<DndPathBuf>, _| Message::DndDropFinished,
                )
                .drag_id(DND_FAVORITES),
                column(active).spacing(app_icon.icon_spacing).into(),
                container(divider::horizontal::default())
                    .width(Length::Fill)
                    .padding([0, divider_padding])
                    .into(),
            )
        };

        let favorites = favorites
            .on_enter(|x, y, _| Message::DndEnter(x, y))
            .on_motion(Message::DndMotion)
            .on_leave(|| Message::DndLeave);

        let show_pinned =
            !self.pinned_list.is_empty() || self.dnd_offer.is_some() || self.is_listening_for_dnd;
        let content_list: Vec<Element<_>> = if show_pinned && !self.active_list.is_empty() {
            vec![favorites.into(), divider, active]
        } else if show_pinned {
            vec![favorites.into()]
        } else if !self.active_list.is_empty() {
            vec![active]
        } else {
            vec![
                icon::from_name("com.system76.CosmicAppList")
                    .size(self.core.applet.suggested_size(false).0)
                    .into(),
            ]
        };

        let mut content = match &self.core.applet.anchor {
            PanelAnchor::Left | PanelAnchor::Right => container(
                Column::with_children(content_list)
                    .spacing(4.0)
                    .align_x(Alignment::Center)
                    .height(h)
                    .width(w),
            ),
            PanelAnchor::Top | PanelAnchor::Bottom => container(
                Row::with_children(content_list)
                    .spacing(4.0)
                    .align_y(Alignment::Center)
                    .height(h)
                    .width(w),
            ),
        };
        if self.active_list.is_empty() && self.pinned_list.is_empty() {
            let suggested_size = self.core.applet.suggested_size(false);
            content = content.width(suggested_size.0).height(suggested_size.1);
        }

        let mut limits = Limits::NONE.min_width(1.).min_height(1.);

        if let Some(b) = self.core.applet.suggested_bounds {
            if b.width as i32 > 0 {
                limits = limits.max_width(b.width);
            }
            if b.height as i32 > 0 {
                limits = limits.max_height(b.height);
            }
        }

        self.core
            .applet
            .autosize_window(content)
            .limits(limits)
            .into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        let theme = self.core.system_theme();

        if let Some((_, item, _, _)) = self.dnd_source.as_ref().filter(|s| s.0 == id) {
            icon_source_with_flatpak_fallback(item.desktop_info.icon().unwrap_or_default())
                .as_cosmic_icon()
                .size(self.core.applet.suggested_size(false).0)
                .into()
        } else if let Some(Popup {
            dock_item: DockItem { id, .. },
            popup_type,
            ..
        }) = self.popup.as_ref().filter(|p| id == p.id)
        {
            let (
                DockItem {
                    toplevels,
                    desktop_info,
                    ..
                },
                is_pinned,
            ) = match self.pinned_list.iter().find(|i| i.id == *id) {
                Some(e) => (e, true),
                None => match self.active_list.iter().find(|i| i.id == *id) {
                    Some(e) => (e, false),
                    None => return text::body("").into(),
                },
            };

            match popup_type {
                PopupType::RightClickMenu => {
                    fn menu_button<'a, Message: Clone + 'a>(
                        content: impl Into<Element<'a, Message>>,
                    ) -> cosmic::widget::Button<'a, Message> {
                        button::custom(content)
                            .height(20 + 2 * theme::active().cosmic().space_xxs())
                            .class(Button::MenuItem)
                            .padding(menu_control_padding())
                            .width(Length::Fill)
                    }

                    let mut content = column![].align_x(Alignment::Center);

                    if let Some(exec) = desktop_info.exec() {
                        if !toplevels.is_empty() {
                            content =
                                content.push(menu_button(text::body(fl!("new-window"))).on_press(
                                    Message::Exec(exec.to_string(), None, desktop_info.terminal()),
                                ));
                        } else if let Some(gpus) = self.gpus.as_ref() {
                            let default_idx = preferred_gpu_idx(desktop_info, gpus.iter());
                            for (i, gpu) in gpus.iter().enumerate() {
                                content = content.push(
                                    menu_button(text::body(format!(
                                        "{} {}",
                                        fl!("run-on", gpu = gpu.name.clone()),
                                        if i == default_idx {
                                            fl!("run-on-default")
                                        } else {
                                            String::new()
                                        }
                                    )))
                                    .on_press(Message::Exec(
                                        exec.to_string(),
                                        Some(i),
                                        desktop_info.terminal(),
                                    )),
                                );
                            }
                        } else {
                            content = content.push(menu_button(text::body(fl!("run"))).on_press(
                                Message::Exec(exec.to_string(), None, desktop_info.terminal()),
                            ));
                        }
                        for action in desktop_info.actions().into_iter().flatten() {
                            if action == "new-window" {
                                continue;
                            }

                            let Some(exec) = desktop_info.action_entry(action, "Exec") else {
                                continue;
                            };
                            let Some(name) =
                                desktop_info.action_entry_localized(action, "Name", &self.locales)
                            else {
                                continue;
                            };
                            content = content.push(menu_button(text::body(name)).on_press(
                                Message::Exec(exec.into(), None, desktop_info.terminal()),
                            ));
                        }
                        content = content.push(divider::horizontal::light());
                    }

                    if !toplevels.is_empty() {
                        let mut list_col = column![];
                        for (info, _) in toplevels {
                            let title = if info.title.len() > 34 {
                                format!("{:.32}...", &info.title)
                            } else {
                                info.title.clone()
                            };
                            list_col =
                                list_col
                                    .push(menu_button(text::body(title)).on_press(
                                        Message::Activate(info.foreign_toplevel.clone()),
                                    ));
                        }
                        content = content.push(list_col);
                        content = content.push(divider::horizontal::light());
                    }

                    let svg_accent = Rc::new(|theme: &cosmic::Theme| {
                        let color = theme.cosmic().accent_color().into();
                        svg::Style { color: Some(color) }
                    });
                    content = content.push(
                        menu_button(
                            if is_pinned {
                                row![
                                    icon::icon(from_name("checkbox-checked-symbolic").into())
                                        .size(16)
                                        .class(cosmic::theme::Svg::Custom(svg_accent.clone())),
                                    text::body(fl!("pin"))
                                ]
                            } else {
                                row![text::body(fl!("pin"))]
                            }
                            .spacing(8),
                        )
                        .on_press(if is_pinned {
                            Message::UnpinApp(*id)
                        } else {
                            Message::PinApp(*id)
                        }),
                    );

                    if !toplevels.is_empty() {
                        content = content.push(divider::horizontal::light());
                        content = match toplevels.len() {
                            1 => content.push(
                                menu_button(text::body(fl!("quit")))
                                    .on_press(Message::Quit(desktop_info.id().to_string())),
                            ),
                            _ => content.push(
                                menu_button(text::body(fl!("quit-all")))
                                    .on_press(Message::Quit(desktop_info.id().to_string())),
                            ),
                        };
                    }
                    self.core
                        .applet
                        .popup_container(
                            container(content)
                                .padding(1)
                                //TODO: move style to libcosmic
                                .class(theme::Container::custom(|theme| {
                                    let cosmic = theme.cosmic();
                                    let component = &cosmic.background.component;
                                    container::Style {
                                        icon_color: Some(component.on.into()),
                                        text_color: Some(component.on.into()),
                                        background: Some(Background::Color(component.base.into())),
                                        border: Border {
                                            radius: cosmic.radius_s().into(),
                                            width: 1.0,
                                            color: component.divider.into(),
                                        },
                                        ..Default::default()
                                    }
                                }))
                                .height(Length::Shrink)
                                .width(Length::Fill),
                        )
                        .limits(
                            Limits::NONE
                                .min_width(1.)
                                .min_height(1.)
                                .max_width(300.)
                                .max_height(1000.),
                        )
                        .into()
                }
                PopupType::TopLevelList => match self.core.applet.anchor {
                    PanelAnchor::Left | PanelAnchor::Right => {
                        let mut content =
                            column![].padding(8).align_x(Alignment::Center).spacing(8);
                        for (info, img) in toplevels {
                            let title = if info.title.len() > 18 {
                                format!("{:.16}...", &info.title)
                            } else {
                                info.title.clone()
                            };
                            content = content.push(toplevel_button(
                                img.clone(),
                                Message::Toggle(info.foreign_toplevel.clone()),
                                title,
                                self.currently_active_toplevel()
                                    .contains(&info.foreign_toplevel),
                            ));
                        }
                        self.core
                            .applet
                            .popup_container(content)
                            .limits(Limits::NONE.min_width(1.).min_height(1.).max_height(1000.))
                            .into()
                    }
                    PanelAnchor::Bottom | PanelAnchor::Top => {
                        let mut content = row![].padding(8).align_y(Alignment::Center).spacing(8);
                        for (info, img) in toplevels {
                            let title = if info.title.len() > 18 {
                                format!("{:.16}...", &info.title)
                            } else {
                                info.title.clone()
                            };
                            content = content.push(toplevel_button(
                                img.clone(),
                                Message::Toggle(info.foreign_toplevel.clone()),
                                title,
                                self.currently_active_toplevel()
                                    .contains(&info.foreign_toplevel),
                            ));
                        }
                        self.core
                            .applet
                            .popup_container(content)
                            .limits(Limits::NONE.min_width(1.).min_height(1.).max_height(1000.))
                            .into()
                    }
                },
            }
        } else if self
            .overflow_active_popup
            .as_ref()
            .is_some_and(|overflow_id| overflow_id == &id)
        {
            let (_favorite_popup_cutoff, active_popup_cutoff) = self.panel_overflow_lengths();

            let focused_item = self.currently_active_toplevel();
            let dot_radius = theme.cosmic().radius_xs();
            // show the overflow popup for active list
            let active: Vec<_> =
                self.active_list[..active_popup_cutoff.map_or(self.active_list.len(), |n| {
                    if n < self.active_list.len() {
                        n.saturating_sub(1)
                    } else {
                        n - 1
                    }
                })]
                    .iter()
                    .map(|dock_item| {
                        self.core
                            .applet
                            .applet_tooltip(
                                dock_item.as_icon(
                                    &self.core.applet,
                                    self.rectangle_tracker.as_ref(),
                                    self.popup.is_none(),
                                    self.config.enable_drag_source,
                                    self.gpus.as_deref(),
                                    dock_item
                                        .toplevels
                                        .iter()
                                        .any(|y| focused_item.contains(&y.0.foreign_toplevel)),
                                    dot_radius,
                                    self.core.main_window_id().unwrap(),
                                ),
                                dock_item
                                    .desktop_info
                                    .full_name(&self.locales)
                                    .unwrap_or_default()
                                    .into_owned(),
                                self.popup.is_some(),
                                Message::Surface,
                                None,
                            )
                            .into()
                    })
                    .collect();
            let content = match &self.core.applet.anchor {
                PanelAnchor::Left | PanelAnchor::Right => container(
                    Column::with_children(active)
                        .spacing(4.0)
                        .align_x(Alignment::Center)
                        .width(Length::Shrink)
                        .height(Length::Shrink),
                ),
                PanelAnchor::Top | PanelAnchor::Bottom => container(
                    Row::with_children(active)
                        .spacing(4.0)
                        .align_y(Alignment::Center)
                        .width(Length::Shrink)
                        .height(Length::Shrink),
                ),
            };
            // send clear popup on press content if there is an active popup
            let content: Element<_> = if self.popup.is_some() {
                mouse_area(content)
                    .on_release(Message::ClosePopup)
                    .on_right_release(Message::ClosePopup)
                    .into()
            } else {
                content.into()
            };
            self.core
                .applet
                .popup_container(content)
                .limits(
                    Limits::NONE
                        .min_width(1.)
                        .min_height(1.)
                        .max_width(1920.)
                        .max_height(1000.),
                )
                .into()
        } else if self
            .overflow_favorites_popup
            .as_ref()
            .is_some_and(|popup_id| popup_id == &id)
        {
            let (favorite_popup_cutoff, _active_popup_cutoff) = self.panel_overflow_lengths();

            let focused_item = self.currently_active_toplevel();
            let dot_radius = theme.cosmic().radius_xs();
            // show the overflow popup for favorites list
            let mut favorite_to_remove = if let Some(cutoff) = favorite_popup_cutoff {
                if cutoff < self.pinned_list.len() {
                    self.pinned_list.len() - cutoff + 1
                } else {
                    0
                }
            } else {
                0
            };
            let mut favorites_extra = Vec::with_capacity(favorite_to_remove);
            let mut favorites: Vec<_> = self
                .pinned_list
                .iter()
                .rev()
                .filter(|f| {
                    if favorite_to_remove > 0 && f.toplevels.is_empty() {
                        favorite_to_remove -= 1;
                        true
                    } else {
                        favorites_extra.push(*f);
                        false
                    }
                })
                .collect();
            favorites.extend(favorites_extra[..favorite_to_remove].iter().copied());
            let favorites: Vec<_> = favorites
                .iter()
                .rev()
                .map(|dock_item| {
                    self.core
                        .applet
                        .applet_tooltip(
                            dock_item.as_icon(
                                &self.core.applet,
                                self.rectangle_tracker.as_ref(),
                                self.popup.is_none(),
                                self.config.enable_drag_source,
                                self.gpus.as_deref(),
                                dock_item
                                    .toplevels
                                    .iter()
                                    .any(|y| focused_item.contains(&y.0.foreign_toplevel)),
                                dot_radius,
                                id,
                            ),
                            dock_item
                                .desktop_info
                                .full_name(&self.locales)
                                .unwrap_or_default()
                                .to_string(),
                            self.popup.is_some(),
                            Message::Surface,
                            Some(id),
                        )
                        .into()
                })
                .collect();
            let content = match &self.core.applet.anchor {
                PanelAnchor::Left | PanelAnchor::Right => container(
                    Column::with_children(favorites)
                        .spacing(4.0)
                        .align_x(Alignment::Center)
                        .width(Length::Shrink)
                        .height(Length::Shrink),
                ),
                PanelAnchor::Top | PanelAnchor::Bottom => container(
                    Row::with_children(favorites)
                        .spacing(4.0)
                        .align_y(Alignment::Center)
                        .width(Length::Shrink)
                        .height(Length::Shrink),
                ),
            };
            let content: Element<_> = if self.popup.is_some() {
                mouse_area(content)
                    .on_right_release(Message::ClosePopup)
                    .on_press(Message::ClosePopup)
                    .into()
            } else {
                content.into()
            };
            self.core
                .applet
                .popup_container(content)
                .limits(
                    Limits::NONE
                        .min_width(1.)
                        .min_height(1.)
                        .max_width(1920.)
                        .max_height(1000.),
                )
                .into()
        } else {
            let suggested = self.core.applet.suggested_size(false);
            iced::widget::row!()
                .width(Length::Fixed(suggested.0 as f32))
                .height(Length::Fixed(suggested.1 as f32))
                .into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            wayland_subscription().map(Message::Wayland),
            listen_with(|e, _, id| match e {
                cosmic::iced_runtime::core::Event::PlatformSpecific(
                    event::PlatformSpecific::Wayland(event::wayland::Event::Seat(e, seat)),
                ) => match e {
                    event::wayland::SeatEvent::Enter => Some(Message::NewSeat(seat)),
                    event::wayland::SeatEvent::Leave => Some(Message::RemovedSeat),
                },
                cosmic::iced_core::Event::Mouse(
                    cosmic::iced_core::mouse::Event::ButtonPressed(_),
                ) => Some(Message::Pressed(id)),
                _ => None,
            }),
            rectangle_tracker_subscription(0).map(|update| Message::Rectangle(update.1)),
            self.core.watch_config(APP_ID).map(|u| {
                for why in u.errors {
                    tracing::error!(why = why.to_string(), "Error watching config");
                }
                Message::ConfigUpdated(u.config)
            }),
        ])
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}

impl CosmicAppList {
    /// Close any open popups.
    fn close_popups(&mut self) -> Task<cosmic::Action<Message>> {
        let mut commands = Vec::new();
        if let Some(popup) = self.popup.take() {
            commands.push(destroy_popup(popup.id));
        }
        if let Some(popup) = self.overflow_active_popup.take() {
            commands.push(destroy_popup(popup));
        }
        if let Some(popup) = self.overflow_favorites_popup.take() {
            commands.push(destroy_popup(popup));
        }
        Task::batch(commands)
    }
    /// Returns the length of the group in the favorite list after which items are displayed in a popup.
    /// Shrink the favorite list until it only has active windows, or until it fits in the length provided.
    fn panel_overflow_lengths(&self) -> (Option<usize>, Option<usize>) {
        let mut favorite_index;
        let mut active_index = None;
        let Some(mut max_major_axis_len) = self.core.applet.suggested_bounds.as_ref().map(|c| {
            // if we have a configure for width and height, we're in a overflow popup
            match self.core.applet.anchor {
                PanelAnchor::Top | PanelAnchor::Bottom => c.width as u32,
                PanelAnchor::Left | PanelAnchor::Right => c.height as u32,
            }
        }) else {
            return (None, active_index);
        };
        // tracing::error!("{} {}", max_major_axis_len, self.pinned_list.len());
        // subtract the divider width
        max_major_axis_len -= 1;
        let applet_icon = AppletIconData::new(&self.core.applet);

        let button_total_size = self.core.applet.suggested_size(true).0
            + self.core.applet.suggested_padding(true).0 * 2
            + applet_icon.icon_spacing as u16;

        let favorite_active_cnt = self
            .pinned_list
            .iter()
            .filter(|t| !t.toplevels.is_empty())
            .count();

        // initial calculation of favorite_index
        let btn_count = max_major_axis_len / button_total_size as u32;
        if btn_count >= self.pinned_list.len() as u32 + self.active_list.len() as u32 {
            return (None, active_index);
        } else {
            favorite_index = (btn_count as usize).min(favorite_active_cnt).max(2);
        }

        // calculation of active_index based on favorite_index if there is still not enough space
        let active_index_max = (btn_count as i32)
            - (self.pinned_list.len() as i32).saturating_sub(favorite_index as i32);
        if active_index_max >= self.active_list.len() as i32 {
            active_index = Some(self.active_list.len());
        } else {
            active_index = Some((active_index_max.max(2) as usize).min(self.active_list.len()));
        }

        // final calculation of favorite_index if there is still not enough space
        if let Some(active_index) = active_index {
            let favorite_index_max = (btn_count as i32) - active_index as i32;
            favorite_index = favorite_index_max.max(2) as usize;
        } else {
            favorite_index = (btn_count as usize).min(self.pinned_list.len());
        }
        // tracing::error!("{} {} {:?}", btn_count, favorite_index, active_index);
        (Some(favorite_index), active_index)
    }

    fn currently_active_toplevel(&self) -> Vec<ExtForeignToplevelHandleV1> {
        if self.active_workspaces.is_empty() {
            return Vec::new();
        }
        let current_output = &self.core.applet.output_name;
        let mut focused_toplevels: Vec<ExtForeignToplevelHandleV1> = Vec::new();
        let active_workspaces = &self.active_workspaces;
        for toplevel_list in self.active_list.iter().chain(self.pinned_list.iter()) {
            for (t_info, _) in &toplevel_list.toplevels {
                if t_info.state.contains(&State::Activated)
                    && active_workspaces
                        .iter()
                        .any(|workspace| t_info.workspace.contains(workspace))
                    && t_info.output.iter().any(|x| {
                        self.output_list.get(x).is_some_and(|val| {
                            val.name.as_ref().is_some_and(|n| n == current_output)
                        })
                    })
                {
                    focused_toplevels.push(t_info.foreign_toplevel.clone());
                }
            }
        }
        focused_toplevels
    }

    fn find_desktop_entry_for_toplevel(
        &mut self,
        info: &ToplevelInfo,
        unicase_appid: Ascii<&str>,
    ) -> DesktopEntry {
        if let Some(appid) = fde::find_app_by_id(&self.desktop_entries, unicase_appid) {
            appid.clone()
        } else {
            // Update desktop entries in case it was not found.

            self.update_desktop_entries();
            if let Some(appid) = fde::find_app_by_id(&self.desktop_entries, unicase_appid) {
                appid.clone()
            } else {
                tracing::error!(id = info.app_id, "could not find desktop entry for app");

                let mut fallback_entry = fde::DesktopEntry::from_appid(info.app_id.clone());

                // proton opens games as steam_app_X, where X is either
                // the steam appid or "default". games with a steam appid
                // can have a desktop entry generated elsewhere; this
                // specifically handles non-steam games opened
                // under proton
                // in addition, try to match WINE entries who have its
                // appid = the full name of the executable (incl. .exe)
                let is_proton_game = info.app_id == "steam_app_default";
                if is_proton_game || info.app_id.ends_with(".exe") {
                    for entry in &self.desktop_entries {
                        let localised_name = entry.name(&self.locales).unwrap_or_default();

                        if localised_name == info.title {
                            // if this is a proton game, we only want
                            // to look for game entries
                            if is_proton_game
                                && !entry.categories().unwrap_or_default().contains(&"Game")
                            {
                                continue;
                            }

                            fallback_entry = entry.clone();
                            break;
                        }
                    }
                }

                fallback_entry
            }
        }
    }
}

fn launch_on_preferred_gpu(desktop_info: &DesktopEntry, gpus: Option<&[Gpu]>) -> Option<Message> {
    let exec = desktop_info.exec()?;

    let gpu_idx = gpus.map(|gpus| preferred_gpu_idx(desktop_info, gpus.iter()));

    Some(Message::Exec(
        exec.to_string(),
        gpu_idx,
        desktop_info.terminal(),
    ))
}

fn preferred_gpu_idx<'a, I>(desktop_info: &DesktopEntry, mut gpus: I) -> usize
where
    I: Iterator<Item = &'a Gpu>,
{
    gpus.position(|gpu| gpu.default ^ desktop_info.prefers_non_default_gpu())
        .unwrap_or(0)
}

#[derive(Debug, Default, Clone)]
pub struct DndPathBuf(PathBuf);

impl AllowedMimeTypes for DndPathBuf {
    fn allowed() -> std::borrow::Cow<'static, [String]> {
        std::borrow::Cow::Owned(vec![MIME_TYPE.to_string()])
    }
}

impl TryFrom<(Vec<u8>, String)> for DndPathBuf {
    type Error = anyhow::Error;

    fn try_from((data, mime_type): (Vec<u8>, String)) -> Result<Self, Self::Error> {
        if mime_type == MIME_TYPE {
            if let Some(p) = String::from_utf8(data)
                .ok()
                .and_then(|s| Url::from_str(&s).ok())
                .and_then(|u| u.to_file_path().ok())
            {
                Ok(DndPathBuf(p))
            } else {
                anyhow::bail!("Failed to parse.")
            }
        } else {
            anyhow::bail!("Invalid mime type.")
        }
    }
}

impl AsMimeTypes for DndPathBuf {
    fn available(&self) -> std::borrow::Cow<'static, [String]> {
        std::borrow::Cow::Owned(vec![MIME_TYPE.to_string()])
    }

    fn as_bytes(&self, _mime_type: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
        Some(Cow::Owned(self.0.to_str()?.as_bytes().to_vec()))
    }
}
