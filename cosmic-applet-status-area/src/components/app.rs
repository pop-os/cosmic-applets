// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    app,
    applet::cosmic_panel_config::PanelAnchor,
    iced::{
        self,
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        window, Limits, Padding, Subscription,
    },
    surface_message::{MessageWrapper, SurfaceMessage},
    widget::{container, mouse_area},
    Element, Task,
};
use std::collections::BTreeMap;

use crate::{components::status_menu, subscriptions::status_notifier_watcher};

#[derive(Clone, Debug)]
pub enum Msg {
    Closed(window::Id),
    // XXX don't use index (unique window id? or I guess that's created and destroyed)
    StatusMenu((usize, status_menu::Msg)),
    StatusNotifier(status_notifier_watcher::Event),
    TogglePopup(usize),
    Hovered(usize),
    Surface(SurfaceMessage),
}

impl From<Msg> for MessageWrapper<Msg> {
    fn from(value: Msg) -> Self {
        match value {
            Msg::Surface(s) => MessageWrapper::Surface(s),
            m => MessageWrapper::Message(m),
        }
    }
}

impl From<SurfaceMessage> for Msg {
    fn from(value: SurfaceMessage) -> Self {
        Msg::Surface(value)
    }
}

#[derive(Default)]
struct App {
    core: app::Core,
    connection: Option<zbus::Connection>,
    menus: BTreeMap<usize, status_menu::State>,
    open_menu: Option<usize>,
    max_menu_id: usize,
    popup: Option<window::Id>,
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
            + self.core.applet.suggested_padding(true) as u32 * 2;
        let n = self.menus.len() as u32;
        window::resize(
            self.core.main_window_id().unwrap(),
            iced::Size::new(1.max(icon_size * n) as f32, icon_size as f32),
        )
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
            Msg::Closed(surface) => {
                if self.popup == Some(surface) {
                    self.popup = None;
                    self.open_menu = None;
                }
                Task::none()
            }
            Msg::StatusMenu((id, msg)) => match self.menus.get_mut(&id) {
                Some(state) => state
                    .update(msg)
                    .map(move |msg| app::message::app(Msg::StatusMenu((id, msg)))),
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
                        return cmd.map(move |msg| app::message::app(Msg::StatusMenu((id, msg))));
                    }
                    let id = self.next_menu_id();
                    self.menus.insert(id, state);
                    app::Task::batch([
                        self.resize_window(),
                        cmd.map(move |msg| app::message::app(Msg::StatusMenu((id, msg)))),
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
                    eprintln!("Status notifier error: {}", err);
                    Task::none()
                }
            },
            Msg::TogglePopup(id) => {
                self.open_menu = if self.open_menu != Some(id) {
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
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        popup_id,
                        None,
                        None,
                        None,
                    );
                    self.popup = Some(popup_id);
                    let i = self.menus.keys().position(|&i| i == id).unwrap();
                    if matches!(
                        self.core.applet.anchor,
                        PanelAnchor::Left | PanelAnchor::Right
                    ) {
                        let suggested_size = self.core.applet.suggested_size(false).1
                            + 2 * self.core.applet.suggested_padding(false);
                        popup_settings.positioner.anchor_rect.y = i as i32 * suggested_size as i32;
                    } else {
                        let suggested_size = self.core.applet.suggested_size(false).0
                            + 2 * self.core.applet.suggested_padding(false);
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
                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    popup_id,
                    None,
                    None,
                    None,
                );
                self.popup = Some(popup_id);
                let i = self.menus.keys().position(|&i| i == id).unwrap();
                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    let suggested_size = self.core.applet.suggested_size(false).1
                        + 2 * self.core.applet.suggested_padding(false);
                    popup_settings.positioner.anchor_rect.y = i as i32 * suggested_size as i32;
                } else {
                    let suggested_size = self.core.applet.suggested_size(false).0
                        + 2 * self.core.applet.suggested_padding(false);
                    popup_settings.positioner.anchor_rect.x = i as i32 * suggested_size as i32;
                }
                cmds.push(get_popup(popup_settings));
                app::Task::batch(cmds)
            }
            Msg::Surface(surface_message) => unreachable!(),
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let mut subscriptions = Vec::new();

        subscriptions.push(status_notifier_watcher::subscription().map(Msg::StatusNotifier));

        for (id, menu) in self.menus.iter() {
            subscriptions.push(menu.subscription().with(*id).map(Msg::StatusMenu));
        }

        iced::Subscription::batch(subscriptions)
    }

    fn view(&self) -> cosmic::Element<'_, Msg> {
        let children = self.menus.iter().map(|(id, menu)| {
            mouse_area(
                match menu.icon_pixmap() {
                    Some(icon) if menu.icon_name() == "" => self
                        .core
                        .applet
                        .icon_button_from_handle(icon.clone().symbolic(true)),
                    _ => self.core.applet.icon_button(menu.icon_name()),
                }
                .on_press_down(Msg::TogglePopup(*id)),
            )
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
                    iced::widget::row(children).into()
                },
            )
            .into()
    }

    fn view_window(&self, _surface: window::Id) -> cosmic::Element<'_, Msg> {
        let theme = self.core.system_theme();
        let cosmic = theme.cosmic();
        let corners = cosmic.corner_radii.clone();
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
                    .limits(Limits::NONE.min_width(1.).min_height(1.).max_width(300.))
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

pub fn main() -> iced::Result {
    cosmic::applet::run::<App>(())
}
