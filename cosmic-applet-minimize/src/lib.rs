// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod localize;
pub(crate) mod wayland_handler;
pub(crate) mod wayland_subscription;
pub(crate) mod window_image;

use std::borrow::Cow;

use crate::localize::localize;
use cosmic::{
    app,
    applet::cosmic_panel_config::PanelAnchor,
    cctk::{
        sctk::reexports::calloop, toplevel_info::ToplevelInfo,
        wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    },
    desktop::fde,
    iced::{
        self,
        id::Id as WidgetId,
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        widget::text,
        window::{self},
        Length, Limits, Subscription,
    },
    surface,
    widget::{autosize::autosize, mouse_area},
    Task,
};

use cosmic::iced_widget::{Column, Row};

use cosmic::{widget::tooltip, Element};
use once_cell::sync::Lazy;
use wayland_subscription::{
    ToplevelRequest, ToplevelUpdate, WaylandImage, WaylandRequest, WaylandUpdate,
};

static AUTOSIZE_MAIN_ID: Lazy<WidgetId> = Lazy::new(|| WidgetId::new("autosize-main"));

pub fn run() -> cosmic::iced::Result {
    localize();
    cosmic::applet::run::<Minimize>(())
}

pub struct App {
    desktop_entry: fde::DesktopEntry,
    name: String,
    icon_source: fde::IconSource,
    toplevel_info: ToplevelInfo,
    wayland_image: Option<WaylandImage>,
}

#[derive(Default)]
struct Minimize {
    core: cosmic::app::Core,
    locales: Vec<String>,
    desktop_entries: Vec<fde::DesktopEntry>,
    apps: Vec<App>,
    tx: Option<calloop::channel::Sender<WaylandRequest>>,
    overflow_popup: Option<window::Id>,
}

impl Minimize {
    fn max_icon_count(&self) -> Option<usize> {
        let mut index = None;
        let Some(max_major_axis_len) = self.core.applet.suggested_bounds.as_ref().map(|c| {
            // if we have a configure for width and height, we're in a overflow popup
            match self.core.applet.anchor {
                PanelAnchor::Top | PanelAnchor::Bottom => c.width as u32,
                PanelAnchor::Left | PanelAnchor::Right => c.height as u32,
            }
        }) else {
            return index;
        };
        let button_total_size = self.core.applet.suggested_size(true).0
            + self.core.applet.suggested_padding(true) * 2
            + 4;
        let btn_count = max_major_axis_len / button_total_size as u32;
        if btn_count >= self.apps.len() as u32 {
            index = None;
        } else {
            index = Some((btn_count as usize).max(2).min(self.apps.len()));
        }
        index
    }

    fn find_new_desktop_entry(&mut self, appid: &str) -> fde::DesktopEntry {
        let unicase_appid = fde::unicase::Ascii::new(appid);

        let de = match fde::find_app_by_id(&self.desktop_entries, unicase_appid) {
            Some(de) => de,
            None => {
                // Update desktop entries in case it was not found.
                self.update_desktop_entries();
                match fde::find_app_by_id(&self.desktop_entries, unicase_appid) {
                    Some(appid) => appid,
                    None => {
                        tracing::warn!(appid, "could not find desktop entry for app");
                        let mut entry = fde::DesktopEntry {
                            appid: appid.to_owned(),
                            groups: Default::default(),
                            path: Default::default(),
                            ubuntu_gettext_domain: None,
                        };
                        entry.add_desktop_entry("Name".to_string(), appid.to_owned());
                        return entry;
                    }
                }
            }
        };

        de.clone()
    }

    // Cache all desktop entries to use when new apps are added to the dock.
    fn update_desktop_entries(&mut self) {
        self.desktop_entries = fde::Iter::new(fde::default_paths())
            .filter_map(|p| fde::DesktopEntry::from_path(p, Some(&self.locales)).ok())
            .collect::<Vec<_>>();
    }
}

#[derive(Debug, Clone)]
enum Message {
    Wayland(WaylandUpdate),
    Activate(ExtForeignToplevelHandleV1),
    Closed(window::Id),
    OpenOverflowPopup,
    CloseOverflowPopup,
    Surface(surface::Action),
}

impl cosmic::Application for Minimize {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletMinimize";

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        let mut app = Self {
            core,
            locales: fde::get_languages_from_env(),
            ..Default::default()
        };

        app.update_desktop_entries();

        (app, Task::none())
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::Wayland(update) => match update {
                WaylandUpdate::Init(tx) => {
                    self.tx = Some(tx);
                }
                WaylandUpdate::Finished => {
                    panic!("Wayland Subscription ended...")
                }
                WaylandUpdate::Toplevel(t) => match t {
                    ToplevelUpdate::Add(toplevel_info) | ToplevelUpdate::Update(toplevel_info) => {
                        // Temporarily take ownership to appease the borrow checker.
                        let mut apps = std::mem::take(&mut self.apps);

                        if let Some(pos) = apps.iter_mut().position(|a| {
                            a.toplevel_info.foreign_toplevel == toplevel_info.foreign_toplevel
                        }) {
                            if apps[pos].toplevel_info.app_id != toplevel_info.app_id {
                                apps[pos].desktop_entry =
                                    self.find_new_desktop_entry(&toplevel_info.app_id);
                                apps[pos].icon_source = fde::IconSource::from_unknown(
                                    apps[pos]
                                        .desktop_entry
                                        .icon()
                                        .unwrap_or(&apps[pos].desktop_entry.appid),
                                )
                            }
                            apps[pos].toplevel_info = toplevel_info;
                        } else {
                            let desktop_entry = self.find_new_desktop_entry(&toplevel_info.app_id);

                            apps.push(App {
                                name: desktop_entry
                                    .full_name(&self.locales)
                                    .unwrap_or(Cow::Borrowed(&desktop_entry.appid))
                                    .to_string(),
                                icon_source: fde::IconSource::from_unknown(
                                    desktop_entry.icon().unwrap_or(&desktop_entry.appid),
                                ),
                                desktop_entry,
                                toplevel_info,
                                wayland_image: None,
                            });
                        }

                        self.apps = apps;
                    }
                    ToplevelUpdate::Remove(handle) => {
                        self.apps
                            .retain(|a| a.toplevel_info.foreign_toplevel != handle);
                        self.apps.shrink_to_fit();
                    }
                },
                WaylandUpdate::Image(handle, img) => {
                    if let Some(pos) = self
                        .apps
                        .iter()
                        .position(|a| a.toplevel_info.foreign_toplevel == handle)
                    {
                        self.apps[pos].wayland_image = Some(img);
                    }
                }
            },
            Message::Activate(handle) => {
                if let Some(tx) = self.tx.as_ref() {
                    let _ = tx.send(WaylandRequest::Toplevel(ToplevelRequest::Activate(handle)));
                }
            }
            Message::Closed(id) => {
                if self.overflow_popup.is_some_and(|i| i == id) {
                    self.overflow_popup = None;
                }
            }
            Message::OpenOverflowPopup => {
                if let Some(id) = self.overflow_popup.take() {
                    return destroy_popup(id);
                } else {
                    let new_id = window::Id::unique();
                    let pos = self.max_icon_count().unwrap_or_default();

                    self.overflow_popup = Some(new_id);
                    let icon_size = self.core.applet.suggested_size(true).0 as u32
                        + 2 * self.core.applet.suggested_padding(true) as u32;
                    let spacing = self.core.system_theme().cosmic().space_xxs() as u32;
                    let major_axis_len = (icon_size + spacing) * (pos.saturating_sub(1) as u32);
                    let rectangle = match self.core.applet.anchor {
                        PanelAnchor::Top | PanelAnchor::Bottom => iced::Rectangle {
                            x: major_axis_len as i32,
                            y: 0,
                            width: icon_size as i32,
                            height: icon_size as i32,
                        },
                        PanelAnchor::Left | PanelAnchor::Right => iced::Rectangle {
                            x: 0,
                            y: major_axis_len as i32,
                            width: icon_size as i32,
                            height: icon_size as i32,
                        },
                    };
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.anchor_rect = rectangle;

                    return get_popup(popup_settings);
                }
            }
            Message::CloseOverflowPopup => todo!(),
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        };
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        wayland_subscription::wayland_subscription().map(Message::Wayland)
    }

    fn view(&self) -> Element<Self::Message> {
        let max_icon_count = self
            .max_icon_count()
            .map(|n| {
                if n < self.apps.len() {
                    n - 1
                } else {
                    self.apps.len()
                }
            })
            .unwrap_or(self.apps.len());
        let (width, _) = self.core.applet.suggested_size(false);
        let padding = self.core.applet.suggested_padding(false);
        let theme = self.core.system_theme().cosmic();
        let space_xxs = theme.space_xxs();
        let icon_buttons = self.apps[..max_icon_count].iter().map(|app| {
            self.core
                .applet
                .applet_tooltip(
                    Element::from(crate::window_image::WindowImage::new(
                        app.wayland_image.clone(),
                        &app.icon_source,
                        width as f32,
                        Message::Activate(app.toplevel_info.foreign_toplevel.clone()),
                        padding,
                    )),
                    app.name.clone(),
                    self.overflow_popup.is_some(),
                    Message::Surface,
                )
                .into()
        });
        let overflow_btn = if max_icon_count < self.apps.len() {
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
                .on_press_down(Message::OpenOverflowPopup);

            Some(btn.into())
        } else {
            None
        };

        // TODO optional dividers on ends if detects app list neighbor
        // not sure the best way to tell if there is an adjacent app-list
        let icon_buttons = icon_buttons.chain(overflow_btn.into_iter());
        let content: Element<_> = if matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        ) {
            Row::with_children(icon_buttons)
                .align_y(cosmic::iced_core::Alignment::Center)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .spacing(space_xxs)
                .into()
        } else {
            Column::with_children(icon_buttons)
                .align_x(cosmic::iced_core::Alignment::Center)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .spacing(space_xxs)
                .into()
        };

        let mut limits = Limits::NONE.min_width(1.).min_height(1.);

        if let Some(b) = self.core.applet.suggested_bounds {
            if b.width as i32 > 0 {
                limits = limits.max_width(b.width);
            }
            if b.height as i32 > 0 {
                limits = limits.max_height(b.height);
            }
        }

        autosize(
            if self.overflow_popup.is_some() {
                mouse_area(content)
                    .on_press(Message::CloseOverflowPopup)
                    .into()
            } else {
                content
            },
            AUTOSIZE_MAIN_ID.clone(),
        )
        .limits(limits)
        .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<Self::Message> {
        let max_icon_count = self
            .max_icon_count()
            .map(|n| {
                if n < self.apps.len() {
                    n - 1
                } else {
                    self.apps.len()
                }
            })
            .unwrap_or(self.apps.len());
        let (width, _) = self.core.applet.suggested_size(false);
        let padding = self.core.applet.suggested_padding(false);
        let theme = self.core.system_theme().cosmic();
        let space_xxs = theme.space_xxs();
        let icon_buttons = self.apps[max_icon_count..].iter().map(|app| {
            tooltip(
                Element::from(crate::window_image::WindowImage::new(
                    app.wayland_image.clone(),
                    &app.icon_source,
                    width as f32,
                    Message::Activate(app.toplevel_info.foreign_toplevel.clone()),
                    padding,
                )),
                text(&app.name).shaping(text::Shaping::Advanced),
                // tooltip::Position::FollowCursor,
                // FIXME tooltip fails to appear when created as indicated in design
                // maybe it should be a subsurface
                match self.core.applet.anchor {
                    PanelAnchor::Left => tooltip::Position::Right,
                    PanelAnchor::Right => tooltip::Position::Left,
                    PanelAnchor::Top => tooltip::Position::Bottom,
                    PanelAnchor::Bottom => tooltip::Position::Top,
                },
            )
            .snap_within_viewport(false)
            .into()
        });

        // TODO optional dividers on ends if detects app list neighbor
        // not sure the best way to tell if there is an adjacent app-list

        self.core
            .applet
            .popup_container(
                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Top | PanelAnchor::Bottom
                ) {
                    Element::from(
                        Row::with_children(icon_buttons)
                            .align_y(cosmic::iced_core::Alignment::Center)
                            .height(Length::Shrink)
                            .width(Length::Shrink)
                            .spacing(space_xxs),
                    )
                } else {
                    Column::with_children(icon_buttons)
                        .align_x(cosmic::iced_core::Alignment::Center)
                        .height(Length::Shrink)
                        .width(Length::Shrink)
                        .spacing(space_xxs)
                        .into()
                },
            )
            .into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Self::Message> {
        Some(Message::Closed(id))
    }
}
