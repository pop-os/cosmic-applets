// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    Element, Task, app,
    applet::cosmic_panel_config::PanelAnchor,
    applet::token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    cctk::sctk::reexports::calloop,
    iced::{
        self, Length, Subscription,
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        window,
    },
    surface,
    widget::{container, mouse_area},
};
use std::collections::BTreeMap;

use crate::{
    components::status_menu,
    subscriptions::{status_notifier_item::StatusNotifierItem, status_notifier_watcher},
};

#[derive(Clone, Debug)]
pub enum Msg {
    None,
    Activate(usize),
    Closed(window::Id),
    // XXX don't use index (unique window id? or I guess that's created and destroyed)
    StatusMenu((usize, status_menu::Msg)),
    StatusNotifier(status_notifier_watcher::Event),
    TogglePopup(usize),
    Hovered(usize),
    Surface(surface::Action),
    ToggleOverflow,
    HoveredOverflow,
    Token(TokenUpdate),
}

#[derive(Default)]
pub(crate) struct App {
    core: app::Core,
    connection: Option<zbus::Connection>,
    menus: BTreeMap<usize, status_menu::State>,
    open_menu: Option<usize>,
    max_menu_id: usize,
    popup: Option<window::Id>,
    overflow_popup: Option<window::Id>,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
}

impl App {
    fn next_menu_id(&mut self) -> usize {
        self.max_menu_id += 1;
        self.max_menu_id
    }

    fn next_popup_id(&mut self) -> window::Id {
        window::Id::unique()
    }

    fn resize_window(&self) -> app::Task<Msg> {
        let icon_size = self.core.applet.suggested_size(true).0 as u32
            + self.core.applet.suggested_padding(true).1 as u32 * 2;
        let n = self.menus.len() as u32;
        window::resize(
            self.core.main_window_id().unwrap(),
            iced::Size::new(1.max(icon_size * n) as f32, icon_size as f32),
        )
    }

    fn overflow_index(&self) -> Option<usize> {
        let max_major_axis_len = self.core.applet.suggested_bounds.as_ref().map(|c| {
            // if we have a configure for width and height, we're in a overflow popup
            match self.core.applet.anchor {
                PanelAnchor::Top | PanelAnchor::Bottom => c.width as u32,
                PanelAnchor::Left | PanelAnchor::Right => c.height as u32,
            }
        })?;

        let button_total_size = self.core.applet.suggested_size(true).0
            + self.core.applet.suggested_padding(true).1 * 2;

        let menu_count = self.menus.len();

        let btn_count = max_major_axis_len / button_total_size as u32;
        if btn_count >= menu_count as u32 {
            None
        } else {
            Some(
                (btn_count.saturating_sub(1) as usize)
                    .min(menu_count)
                    .max(1),
            )
        }
    }

    fn view_overflow_popup(&self) -> cosmic::Element<'_, Msg> {
        // Render the overflow popup with the menus that are not shown in the main view
        let overflow_index = self.overflow_index().unwrap_or(0);
        let children = self.menus.iter().skip(overflow_index).map(|(id, menu)| {
            mouse_area(
                menu_icon_button(&self.core.applet, &menu).on_press_down(Msg::TogglePopup(*id)),
            )
            .on_enter(Msg::Hovered(*id))
            .into()
        });

        self.core
            .applet
            .popup_container(container(
                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    Element::from(iced::widget::column(children))
                } else {
                    Element::from(iced::widget::row(children))
                },
            ))
            .into()
    }
}

impl cosmic::Application for App {
    type Message = Msg;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletStatusArea";

    fn init(core: app::Core, _flags: ()) -> (Self, app::Task<Msg>) {
        (
            Self {
                core,
                ..Self::default()
            },
            Task::none(),
        )
    }

    fn core(&self) -> &app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Msg) -> app::Task<Msg> {
        match message {
            Msg::None => Task::none(),
            Msg::Activate(id) => {
                if let Some(token_tx) = self.token_tx.as_ref() {
                    let _ = token_tx.send(TokenRequest {
                        app_id: Self::APP_ID.to_string(),
                        exec: format!("activate:{}", id),
                    });
                } else {
                    if let Some(menu) = self.menus.get(&id) {
                        return activate(id, &menu.item, None);
                    }
                }
                Task::none()
            }
            Msg::Closed(surface) => {
                if self.popup == Some(surface) {
                    self.popup = None;
                    self.open_menu = None;
                }
                Task::none()
            }
            Msg::StatusMenu((id, msg)) => match self.menus.get_mut(&id) {
                Some(state) => state
                    .update(msg, id, self.token_tx.as_ref())
                    .map(move |msg| cosmic::action::app(Msg::StatusMenu((id, msg)))),
                None => Task::none(),
            },
            Msg::StatusNotifier(event) => match event {
                status_notifier_watcher::Event::Connected(connection) => {
                    self.connection = Some(connection);
                    Task::none()
                }
                status_notifier_watcher::Event::Registered(name) => {
                    let (state, cmd) = status_menu::State::new(name);
                    if let Some((id, m)) = self
                        .menus
                        .iter_mut()
                        .find(|(_, prev_state)| prev_state.name() == state.name())
                    {
                        *m = state;
                        let id = *id;
                        return cmd.map(move |msg| cosmic::action::app(Msg::StatusMenu((id, msg))));
                    }
                    let id = self.next_menu_id();
                    self.menus.insert(id, state);
                    app::Task::batch([
                        self.resize_window(),
                        cmd.map(move |msg| cosmic::action::app(Msg::StatusMenu((id, msg)))),
                    ])
                }
                status_notifier_watcher::Event::Unregistered(name) => {
                    if let Some((id, _)) = self.menus.iter().find(|(_id, menu)| menu.name() == name)
                    {
                        let id = *id;
                        self.menus.remove(&id);
                        if self.open_menu == Some(id) {
                            self.open_menu = None;
                            if let Some(popup_id) = self.popup {
                                return destroy_popup(popup_id);
                            }
                        }
                    }
                    self.resize_window()
                }
                status_notifier_watcher::Event::Error(err) => {
                    eprintln!("Status notifier error: {err}");
                    Task::none()
                }
            },
            Msg::TogglePopup(id) => {
                self.open_menu = if self.open_menu.is_none() {
                    Some(id)
                } else {
                    None
                };
                if self.open_menu.is_some() {
                    self.menus[&id].opened();

                    let mut cmds = Vec::new();
                    if let Some(popup_id) = self.popup.take() {
                        cmds.push(destroy_popup(popup_id));
                    }
                    let popup_id = self.next_popup_id();
                    let i = self.menus.keys().position(|&i| i == id).unwrap();
                    let (i, parent) = self
                        .overflow_index()
                        .and_then(|overflow_i| {
                            if overflow_i <= i {
                                Some(i - overflow_i).zip(self.overflow_popup)
                            } else {
                                Some((i, self.core.main_window_id().unwrap()))
                            }
                        })
                        .unwrap_or((i, self.core.main_window_id().unwrap()));

                    let mut popup_settings = self
                        .core
                        .applet
                        .get_popup_settings(parent, popup_id, None, None, None);
                    self.popup = Some(popup_id);

                    if matches!(
                        self.core.applet.anchor,
                        PanelAnchor::Left | PanelAnchor::Right
                    ) {
                        let suggested_size = self.core.applet.suggested_size(false).1
                            + 2 * self.core.applet.suggested_padding(false).1;
                        popup_settings.positioner.anchor_rect.y = i as i32 * suggested_size as i32;
                    } else {
                        let suggested_size = self.core.applet.suggested_size(false).0
                            + 2 * self.core.applet.suggested_padding(false).1;
                        popup_settings.positioner.anchor_rect.x = i as i32 * suggested_size as i32;
                    }
                    cmds.push(get_popup(popup_settings));
                    return app::Task::batch(cmds);
                } else if let Some(popup_id) = self.popup {
                    self.menus[&id].closed();

                    return destroy_popup(popup_id);
                }
                Task::none()
            }
            Msg::Token(u) => match u {
                TokenUpdate::Init(tx) => {
                    self.token_tx = Some(tx);
                    return Task::none();
                }
                TokenUpdate::Finished => {
                    self.token_tx = None;
                    return Task::none();
                }
                TokenUpdate::ActivationToken { token, exec: id } => {
                    if let Some(id_str) = id.strip_prefix("activate:") {
                        if let Ok(real_id) = id_str.parse::<usize>() {
                            if let Some(menu) = self.menus.get(&real_id) {
                                return activate(real_id, &menu.item, token.clone());
                            }
                        }
                        return Task::none();
                    }
                    if let Some(((state, id), token)) = str::parse(&id)
                        .ok()
                        .and_then(|id: usize| self.menus.get_mut(&id).map(|m| (m, id)))
                        .zip(token)
                    {
                        return state
                            .update(
                                status_menu::Msg::ClickToken(token),
                                id,
                                self.token_tx.as_ref(),
                            )
                            .map(move |msg| cosmic::action::app(Msg::StatusMenu((id, msg))));
                    }
                    return Task::none();
                }
            },
            Msg::Hovered(id) => {
                let mut cmds = Vec::new();
                if let Some(old_id) = self.open_menu.take() {
                    if old_id != id {
                        if let Some(popup_id) = self.popup.take() {
                            cmds.push(destroy_popup(popup_id));
                        }
                        self.open_menu = Some(id);
                    } else {
                        self.open_menu = Some(old_id);
                        return Task::none();
                    }
                } else {
                    return Task::none();
                }
                let popup_id = self.next_popup_id();
                let i = self.menus.keys().position(|&i| i == id).unwrap();

                let (i, parent) = self
                    .overflow_index()
                    .and_then(|overflow_i| {
                        if overflow_i <= i {
                            Some(i - overflow_i).zip(self.overflow_popup)
                        } else {
                            Some((i, self.core.main_window_id().unwrap()))
                        }
                    })
                    .unwrap_or((i, self.core.main_window_id().unwrap()));

                let mut popup_settings = self
                    .core
                    .applet
                    .get_popup_settings(parent, popup_id, None, None, None);
                self.popup = Some(popup_id);

                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    let suggested_size = self.core.applet.suggested_size(true).1
                        + 2 * self.core.applet.suggested_padding(true).1;
                    popup_settings.positioner.anchor_rect.y = i as i32 * suggested_size as i32;
                } else {
                    let suggested_size = self.core.applet.suggested_size(true).0
                        + 2 * self.core.applet.suggested_padding(true).0;
                    popup_settings.positioner.anchor_rect.x = i as i32 * suggested_size as i32;
                }
                cmds.push(get_popup(popup_settings));
                Task::batch(cmds)
            }
            Msg::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Msg::ToggleOverflow => {
                if let Some(popup_id) = self.overflow_popup.take() {
                    self.popup = None;
                    self.open_menu = None;
                    return destroy_popup(popup_id);
                } else if let Some(overflow_index) = self.overflow_index() {
                    // If we don't have an overflow, create it
                    let popup_id = self.next_popup_id();
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        popup_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.close_with_children = false;

                    if matches!(
                        self.core.applet.anchor,
                        PanelAnchor::Left | PanelAnchor::Right
                    ) {
                        let suggested_size = self.core.applet.suggested_size(true).1
                            + 2 * self.core.applet.suggested_padding(true).1;
                        popup_settings.positioner.anchor_rect.y =
                            overflow_index as i32 * suggested_size as i32;
                    } else {
                        let suggested_size = self.core.applet.suggested_size(true).0
                            + 2 * self.core.applet.suggested_padding(true).0;
                        popup_settings.positioner.anchor_rect.x =
                            overflow_index as i32 * suggested_size as i32;
                    }

                    self.overflow_popup = Some(popup_id);
                    return get_popup(popup_settings);
                } else {
                    return Task::none();
                }
            }
            Msg::HoveredOverflow => {
                let mut cmds = Vec::new();
                if self.overflow_popup.is_some() {
                    // If we already have an overflow popup, do nothing
                    return Task::none();
                } else if self.open_menu.is_some() {
                    // If we have an open menu, close it
                    if let Some(popup_id) = self.popup.take() {
                        cmds.push(destroy_popup(popup_id));
                    }
                } else {
                    return Task::none();
                }

                let popup_id = self.next_popup_id();
                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    popup_id,
                    None,
                    None,
                    None,
                );
                self.popup = Some(popup_id);

                let Some(i) = self.overflow_index() else {
                    return Task::batch(cmds);
                };

                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    let suggested_size = self.core.applet.suggested_size(false).1
                        + 2 * self.core.applet.suggested_padding(false).1;
                    popup_settings.positioner.anchor_rect.y = i as i32 * suggested_size as i32;
                } else {
                    let suggested_size = self.core.applet.suggested_size(false).0
                        + 2 * self.core.applet.suggested_padding(false).1;
                    popup_settings.positioner.anchor_rect.x = i as i32 * suggested_size as i32;
                }
                cmds.push(get_popup(popup_settings));
                Task::batch(cmds)
            }
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let mut subscriptions = Vec::new();

        subscriptions.push(status_notifier_watcher::subscription().map(Msg::StatusNotifier));

        for (id, menu) in &self.menus {
            subscriptions.push(menu.subscription().with(*id).map(Msg::StatusMenu));
        }
        subscriptions.push(activation_token_subscription(0).map(Msg::Token));

        iced::Subscription::batch(subscriptions)
    }

    fn view(&self) -> cosmic::Element<'_, Msg> {
        let overflow_index = self.overflow_index();

        let children = self
            .menus
            .iter()
            .take(overflow_index.unwrap_or(self.menus.len()))
            .map(|(id, menu)| {
                mouse_area(menu_icon_button(&self.core.applet, &menu).on_press(Msg::Activate(*id)))
                    .on_right_press(Msg::TogglePopup(*id))
                    .on_enter(Msg::Hovered(*id))
                    .into()
            });

        self.core
            .applet
            .autosize_window(
                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    Element::from(iced::widget::column(children))
                } else {
                    iced::widget::row(children)
                        .push_maybe(overflow_index.map(|_| {
                            mouse_area(
                                self.core
                                    .applet
                                    .icon_button(match self.core.applet.anchor {
                                        PanelAnchor::Bottom => "go-up-symbolic",
                                        PanelAnchor::Left => "go-next-symbolic",
                                        PanelAnchor::Right => "go-previous-symbolic",
                                        PanelAnchor::Top => "go-down-symbolic",
                                    })
                                    .on_press_down(Msg::ToggleOverflow),
                            )
                            .on_enter(Msg::HoveredOverflow)
                        }))
                        .into()
                },
            )
            .into()
    }

    fn view_window(&self, surface: window::Id) -> cosmic::Element<'_, Msg> {
        if self
            .overflow_popup
            .as_ref()
            .is_some_and(|id| *id == surface)
        {
            return self.view_overflow_popup();
        }

        let theme = self.core.system_theme();
        let cosmic = theme.cosmic();
        let corners = cosmic.corner_radii;
        let pad = corners.radius_m[0];
        match self.open_menu {
            Some(id) => match self.menus.get(&id) {
                Some(menu) => self
                    .core
                    .applet
                    .popup_container(
                        container(menu.popup_view().map(move |msg| Msg::StatusMenu((id, msg))))
                            .padding([pad, 0.]),
                    )
                    .into(),
                None => unreachable!(),
            },
            None => iced::widget::text("").into(),
        }
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Msg> {
        Some(Msg::Closed(id))
    }
}

fn activate(
    id: usize,
    item: &StatusNotifierItem,
    activation_token: Option<String>,
) -> Task<cosmic::Action<Msg>> {
    let item_proxy = item.item_proxy().clone();
    Task::future(async move {
        if let Some(t) = activation_token {
            match item_proxy.provide_xdg_activation_token(t).await {
                Ok(_) => {
                    tracing::debug!("Token provided successfully to {}", id)
                }
                Err(e) => tracing::error!("Failed to provide token to {}: {}", id, e),
            }
        }
        match item_proxy.activate(0, 0).await {
            Ok(_) => cosmic::action::app(Msg::None),
            Err(err) => {
                tracing::error!("Activate failed: {}", err);
                cosmic::action::app(Msg::TogglePopup(id))
            }
        }
    })
}

fn menu_icon_button<'a>(
    applet: &'a cosmic::applet::Context,
    menu: &'a status_menu::State,
) -> cosmic::widget::Button<'a, Msg> {
    let icon = menu.icon_handle().clone();

    let theme = cosmic::theme::active();
    let theme = theme.cosmic();

    let suggested = applet.suggested_size(true);
    let padding = applet.suggested_padding(true).1;
    // let (major_padding, applet_padding_minor_axis) = applet.suggested_padding(true);
    // let (horizontal_padding, vertical_padding) = if applet.is_horizontal() {
    //     (major_padding, applet_padding_minor_axis)
    // } else {
    //     (applet_padding_minor_axis, major_padding)
    // };
    let symbolic = icon.symbolic;

    cosmic::widget::button::custom(
        cosmic::widget::layer_container(
            cosmic::widget::icon(icon)
                .class(if symbolic {
                    cosmic::theme::Svg::Custom(std::rc::Rc::new(|theme| {
                        cosmic::iced_widget::svg::Style {
                            color: Some(theme.cosmic().background.on.into()),
                        }
                    }))
                } else {
                    cosmic::theme::Svg::default()
                })
                .width(Length::Fixed(suggested.0 as f32))
                .height(Length::Fixed(suggested.1 as f32)),
        )
        .center(Length::Fill),
    )
    .width(Length::Fixed((suggested.0 + 2 * padding) as f32))
    .height(Length::Fixed((suggested.1 + 2 * padding) as f32))
    .class(cosmic::theme::Button::AppletIcon)
}

pub fn main() -> iced::Result {
    cosmic::applet::run::<App>(())
}
