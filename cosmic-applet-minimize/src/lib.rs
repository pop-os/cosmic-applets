// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod localize;
pub(crate) mod wayland_handler;
pub(crate) mod wayland_subscription;
pub(crate) mod window_image;

use crate::localize::localize;
use cosmic::{
    app::Command,
    applet::cosmic_panel_config::PanelAnchor,
    cctk::{
        cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        sctk::reexports::calloop, toplevel_info::ToplevelInfo,
    },
    desktop::DesktopEntryData,
    iced::{
        self,
        wayland::popup::{destroy_popup, get_popup},
        widget::text,
        window::{self},
        Length, Subscription,
    },
    widget::mouse_area,
};

use cosmic::{
    iced_style::application,
    iced_widget::{Column, Row},
};

use cosmic::{widget::tooltip, Element, Theme};
use wayland_subscription::{
    ToplevelRequest, ToplevelUpdate, WaylandImage, WaylandRequest, WaylandUpdate,
};

pub fn run() -> cosmic::iced::Result {
    localize();
    cosmic::applet::run::<Minimize>(true, ())
}

#[derive(Default)]
struct Minimize {
    core: cosmic::app::Core,
    apps: Vec<(
        ZcosmicToplevelHandleV1,
        ToplevelInfo,
        DesktopEntryData,
        Option<WaylandImage>,
    )>,
    tx: Option<calloop::channel::Sender<WaylandRequest>>,
    overflow_popup: Option<window::Id>,
}

impl Minimize {
    fn max_icon_count(&self) -> Option<usize> {
        let mut index = None;
        let Some(max_major_axis_len) = self.core.applet.configure.as_ref().and_then(|c| {
            // if we have a configure for width and height, we're in a overflow popup
            match self.core.applet.anchor {
                PanelAnchor::Top | PanelAnchor::Bottom => c.new_size.0,
                PanelAnchor::Left | PanelAnchor::Right => c.new_size.1,
            }
        }) else {
            return index;
        };
        let button_total_size = self.core.applet.suggested_size(true).0
            + self.core.applet.suggested_padding(true) * 2
            + 4;
        let btn_count = max_major_axis_len.get() / button_total_size as u32;
        if btn_count >= self.apps.len() as u32 {
            index = None;
        } else {
            index = Some((btn_count as usize).max(2).min(self.apps.len()));
        }
        index
    }
}

#[derive(Debug, Clone)]
enum Message {
    Wayland(WaylandUpdate),
    Activate(ZcosmicToplevelHandleV1),
    Closed(window::Id),
    OpenOverflowPopup,
    CloseOverflowPopup,
}

impl cosmic::Application for Minimize {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletMinimize";

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                core,
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Wayland(update) => match update {
                WaylandUpdate::Init(tx) => {
                    self.tx = Some(tx);
                }
                WaylandUpdate::Finished => {
                    panic!("Wayland Subscription ended...")
                }
                WaylandUpdate::Toplevel(t) => match t {
                    ToplevelUpdate::Add(handle, info) | ToplevelUpdate::Update(handle, info) => {
                        let data = |id| {
                            cosmic::desktop::load_applications_for_app_ids(
                                None,
                                std::iter::once(id),
                                true,
                                false,
                            )
                            .remove(0)
                        };
                        if let Some(pos) = self.apps.iter_mut().position(|a| a.0 == handle) {
                            if self.apps[pos].1.app_id != info.app_id {
                                self.apps[pos].2 = data(&info.app_id)
                            }
                            self.apps[pos].1 = info;
                        } else {
                            let data = data(&info.app_id);
                            self.apps.push((handle, info, data, None));
                        }
                    }
                    ToplevelUpdate::Remove(handle) => self.apps.retain(|a| a.0 != handle),
                },
                WaylandUpdate::Image(handle, img) => {
                    if let Some(pos) = self.apps.iter().position(|a| a.0 == handle) {
                        self.apps[pos].3 = Some(img);
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
                        window::Id::MAIN,
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
        };
        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        wayland_subscription::wayland_subscription().map(Message::Wayland)
    }

    fn view(&self) -> Element<Message> {
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
        let icon_buttons = self.apps[..max_icon_count]
            .iter()
            .map(|(handle, _, data, img)| {
                tooltip(
                    Element::from(crate::window_image::WindowImage::new(
                        img.clone(),
                        &data.icon,
                        width as f32,
                        Message::Activate(handle.clone()),
                        padding,
                    )),
                    data.name.clone(),
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
                .text_shaping(text::Shaping::Advanced)
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
                .on_press(Message::OpenOverflowPopup);

            Some(btn.into())
        } else {
            None
        };

        // TODO optional dividers on ends if detects app list neighbor
        // not sure the best way to tell if there is an adjacent app-list
        let icon_buttons = icon_buttons.chain(overflow_btn.into_iter());
        let content = if matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        ) {
            Row::with_children(icon_buttons)
                .align_items(cosmic::iced_core::Alignment::Center)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .spacing(space_xxs)
                .into()
        } else {
            Column::with_children(icon_buttons)
                .align_items(cosmic::iced_core::Alignment::Center)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .spacing(space_xxs)
                .into()
        };
        if self.overflow_popup.is_some() {
            mouse_area(content)
                .on_press(Message::CloseOverflowPopup)
                .into()
        } else {
            content
        }
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
        let icon_buttons = self.apps[max_icon_count..]
            .iter()
            .map(|(handle, _, data, img)| {
                tooltip(
                    Element::from(crate::window_image::WindowImage::new(
                        img.clone(),
                        &data.icon,
                        width as f32,
                        Message::Activate(handle.clone()),
                        padding,
                    )),
                    data.name.clone(),
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
                .text_shaping(text::Shaping::Advanced)
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
                            .align_items(cosmic::iced_core::Alignment::Center)
                            .height(Length::Shrink)
                            .width(Length::Shrink)
                            .spacing(space_xxs),
                    )
                } else {
                    Column::with_children(icon_buttons)
                        .align_items(cosmic::iced_core::Alignment::Center)
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
