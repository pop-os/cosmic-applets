// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cctk::{
    sctk::reexports::{
        calloop::channel::SyncSender,
        protocols::ext::workspace::v1::client::ext_workspace_handle_v1::{
            self, ExtWorkspaceHandleV1,
        },
    },
    workspace::Workspace,
};
use cosmic::{
    Element, Task, Theme, app,
    applet::cosmic_panel_config::PanelAnchor,
    iced::{
        Alignment,
        Event::Mouse,
        Length, Limits, Subscription, event,
        mouse::{self, ScrollDelta},
        widget::{button, column, row},
    },
    iced_core::{Background, Border},
    surface,
    widget::{Id, autosize, container, horizontal_space, vertical_space},
};

use crate::{
    config,
    wayland::WorkspaceEvent,
    wayland_subscription::{WorkspacesUpdate, workspaces},
};

use std::{
    process::Command as ShellCommand,
    sync::LazyLock,
    time::{Duration, Instant},
};

static AUTOSIZE_MAIN_ID: LazyLock<Id> = LazyLock::new(|| Id::new("autosize-main"));

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<IcedWorkspacesApplet>(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Row,
    Column,
}

struct IcedWorkspacesApplet {
    core: cosmic::app::Core,
    workspaces: Vec<Workspace>,
    workspace_tx: Option<SyncSender<WorkspaceEvent>>,
    layout: Layout,
    scroll: f64,
    next_scroll: Option<Instant>,
    last_scroll: Instant,
}

impl IcedWorkspacesApplet {
    /// returns the index of the workspace button after which which must be moved to a popup
    /// if it exists.
    fn popup_index(&self) -> Option<usize> {
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
        if btn_count >= self.workspaces.len() as u32 {
            index = None;
        } else {
            index = Some((btn_count as usize).min(self.workspaces.len()));
        }
        index
    }
}

#[derive(Debug, Clone)]
enum Message {
    WorkspaceUpdate(WorkspacesUpdate),
    WorkspacePressed(ExtWorkspaceHandleV1),
    WheelScrolled(ScrollDelta),
    WorkspaceOverview,
    Surface(surface::Action),
}

impl cosmic::Application for IcedWorkspacesApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        (
            Self {
                layout: match &core.applet.anchor {
                    PanelAnchor::Left | PanelAnchor::Right => Layout::Column,
                    PanelAnchor::Top | PanelAnchor::Bottom => Layout::Row,
                },
                core,
                workspaces: Vec::new(),
                workspace_tx: Default::default(),
                scroll: 0.0,
                next_scroll: None,
                last_scroll: Instant::now(),
            },
            Task::none(),
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
            Message::WorkspaceUpdate(msg) => match msg {
                WorkspacesUpdate::Workspaces(mut list) => {
                    list.retain(|w| !w.state.contains(ext_workspace_handle_v1::State::Hidden));
                    list.sort_by(|w1, w2| w1.coordinates.cmp(&w2.coordinates));
                    self.workspaces = list;
                }
                WorkspacesUpdate::Started(tx) => {
                    self.workspace_tx.replace(tx);
                }
                WorkspacesUpdate::Errored => {
                    // TODO
                }
            },
            Message::WorkspacePressed(id) => {
                if let Some(tx) = self.workspace_tx.as_mut() {
                    let _ = tx.try_send(WorkspaceEvent::Activate(id));
                }
            }
            Message::WheelScrolled(delta) => {
                let (delta, debounce) = match delta {
                    ScrollDelta::Lines { x, y } => ((x + y) as f64, false),
                    ScrollDelta::Pixels { x, y } => ((x + y) as f64, true),
                };

                let dur = if debounce {
                    Duration::from_millis(350)
                } else {
                    Duration::from_millis(200)
                };
                if self.last_scroll.elapsed() > Duration::from_millis(100)
                    || self.scroll * delta < 0.0
                {
                    self.next_scroll = None;
                    self.scroll = 0.0;
                }
                self.last_scroll = Instant::now();

                self.scroll += delta;
                if let Some(next) = self.next_scroll {
                    if next > Instant::now() {
                        return cosmic::iced::Task::none();
                    }
                    self.next_scroll = None;
                }

                if self.scroll.abs() < 1.0 {
                    return cosmic::iced::Task::none();
                }
                self.next_scroll = Some(Instant::now() + dur);
                if let Some(w_i) = self
                    .workspaces
                    .iter()
                    .position(|w| w.state.contains(ext_workspace_handle_v1::State::Active))
                {
                    let max_w = self.workspaces.len().wrapping_sub(1);
                    let d_i = if self.scroll > 0.0 {
                        if w_i == 0 { max_w } else { w_i.wrapping_sub(1) }
                    } else if w_i == max_w {
                        0
                    } else {
                        w_i.wrapping_add(1)
                    };
                    self.scroll = 0.0;
                    if let Some(w) = self.workspaces.get(d_i) {
                        if let Some(tx) = self.workspace_tx.as_mut() {
                            let _ = tx.try_send(WorkspaceEvent::Activate(w.handle.clone()));
                        }
                    }
                }
            }
            Message::WorkspaceOverview => {
                let _ = ShellCommand::new("cosmic-workspaces").spawn();
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Message> {
        if self.workspaces.is_empty() {
            return row![].padding(8).into();
        }
        let horizontal = matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        );
        let suggested_total =
            self.core.applet.suggested_size(true).0 + self.core.applet.suggested_padding(true) * 2;
        let suggested_window_size = self.core.applet.suggested_window_size();
        let popup_index = self.popup_index().unwrap_or(self.workspaces.len());

        let buttons = self.workspaces[..popup_index].iter().filter_map(|w| {
            let content = self.core.applet.text(&w.name).font(cosmic::font::bold());

            let (width, height) = if self.core.applet.is_horizontal() {
                (suggested_total as f32, suggested_window_size.1.get() as f32)
            } else {
                (suggested_window_size.0.get() as f32, suggested_total as f32)
            };

            let content = row!(content, vertical_space().height(Length::Fixed(height)))
                .align_y(Alignment::Center);

            let content = column!(content, horizontal_space().width(Length::Fixed(width)))
                .align_x(Alignment::Center);

            let btn = button(
                container(content)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .padding(if horizontal {
                [0, self.core.applet.suggested_padding(true)]
            } else {
                [self.core.applet.suggested_padding(true), 0]
            })
            .on_press(
                if w.state.contains(ext_workspace_handle_v1::State::Active) {
                    Message::WorkspaceOverview
                } else {
                    Message::WorkspacePressed(w.handle.clone())
                },
            )
            .padding(0);

            Some(
                btn.class(
                    if w.state.contains(ext_workspace_handle_v1::State::Active) {
                        cosmic::theme::iced::Button::Primary
                    } else if w.state.contains(ext_workspace_handle_v1::State::Urgent) {
                        let appearance = |theme: &Theme| {
                            let cosmic = theme.cosmic();
                            button::Style {
                                background: Some(Background::Color(
                                    cosmic.palette.neutral_3.into(),
                                )),
                                border: Border {
                                    radius: cosmic.radius_xl().into(),
                                    ..Default::default()
                                },
                                border_radius: theme.cosmic().radius_xl().into(),
                                text_color: theme.cosmic().destructive_button.base.into(),
                                ..button::Style::default()
                            }
                        };
                        cosmic::theme::iced::Button::Custom(Box::new(move |theme, status| {
                            match status {
                                button::Status::Active => appearance(theme),
                                button::Status::Hovered => button::Style {
                                    background: Some(Background::Color(
                                        theme.current_container().component.hover.into(),
                                    )),
                                    border: Border {
                                        radius: theme.cosmic().radius_xl().into(),
                                        ..Default::default()
                                    },
                                    ..appearance(theme)
                                },
                                button::Status::Pressed => appearance(theme),
                                button::Status::Disabled => appearance(theme),
                            }
                        }))
                    } else {
                        let appearance = |theme: &Theme| {
                            let cosmic = theme.cosmic();
                            button::Style {
                                background: None,
                                border: Border {
                                    radius: cosmic.radius_xl().into(),
                                    ..Default::default()
                                },
                                border_radius: cosmic.radius_xl().into(),
                                text_color: theme.current_container().component.on.into(),
                                ..button::Style::default()
                            }
                        };
                        cosmic::theme::iced::Button::Custom(Box::new(move |theme, status| {
                            match status {
                                button::Status::Active => appearance(theme),
                                button::Status::Hovered => button::Style {
                                    background: Some(Background::Color(
                                        theme.current_container().component.hover.into(),
                                    )),
                                    border: Border {
                                        radius: theme.cosmic().radius_xl().into(),
                                        ..Default::default()
                                    },
                                    ..appearance(theme)
                                },
                                button::Status::Pressed | button::Status::Disabled => {
                                    appearance(theme)
                                }
                            }
                        }))
                    },
                )
                .into(),
            )
        });
        // TODO if there is a popup_index, create a button with a popup for the remaining workspaces
        // Should it appear on hover or on click?
        let layout_section: Element<_> = match self.layout {
            Layout::Row => row(buttons).spacing(4).into(),
            Layout::Column => column(buttons).spacing(4).into(),
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

        autosize::autosize(
            container(layout_section).padding(0),
            AUTOSIZE_MAIN_ID.clone(),
        )
        .limits(limits)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            workspaces().map(Message::WorkspaceUpdate),
            event::listen_with(|e, _, _| match e {
                Mouse(mouse::Event::WheelScrolled { delta }) => Some(Message::WheelScrolled(delta)),
                _ => None,
            }),
        ])
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
