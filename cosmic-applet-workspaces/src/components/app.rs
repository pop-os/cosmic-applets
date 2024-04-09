use cctk::sctk::reexports::{calloop::channel::SyncSender, client::backend::ObjectId};
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::event;
use cosmic::iced::mouse::{self, ScrollDelta};
use cosmic::iced::widget::{button, column, container, row, text};
use cosmic::iced::{Event::Mouse, Length, Subscription};
use cosmic::iced_core::{Background, Border};
use cosmic::iced_style::application;
use cosmic::widget::{horizontal_space, vertical_space};
use cosmic::{applet::cosmic_panel_config::PanelAnchor, font::FONT_BOLD, Command};
use cosmic::{Element, Theme};

use cosmic_protocols::workspace::v1::client::zcosmic_workspace_handle_v1;
use std::cmp::Ordering;

use crate::config;
use crate::wayland::{WorkspaceEvent, WorkspaceList};
use crate::wayland_subscription::{workspaces, WorkspacesUpdate};

use std::process::Command as ShellCommand;

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<IcedWorkspacesApplet>(true, ())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Row,
    Column,
}

struct IcedWorkspacesApplet {
    core: cosmic::app::Core,
    workspaces: WorkspaceList,
    workspace_tx: Option<SyncSender<WorkspaceEvent>>,
    layout: Layout,
}

#[derive(Debug, Clone)]
enum Message {
    WorkspaceUpdate(WorkspacesUpdate),
    WorkspacePressed(ObjectId),
    WheelScrolled(ScrollDelta),
    WorkspaceOverview,
}

impl cosmic::Application for IcedWorkspacesApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (
        Self,
        cosmic::iced::Command<cosmic::app::Message<Self::Message>>,
    ) {
        (
            Self {
                layout: match &core.applet.anchor {
                    PanelAnchor::Left | PanelAnchor::Right => Layout::Column,
                    PanelAnchor::Top | PanelAnchor::Bottom => Layout::Row,
                },
                core,
                workspaces: Vec::new(),
                workspace_tx: Default::default(),
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

    fn update(
        &mut self,
        message: Self::Message,
    ) -> cosmic::iced::Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::WorkspaceUpdate(msg) => match msg {
                WorkspacesUpdate::Workspaces(mut list) => {
                    list.retain(|w| {
                        !matches!(w.1, Some(zcosmic_workspace_handle_v1::State::Hidden))
                    });
                    list.sort_by(|a, b| match a.0.len().cmp(&b.0.len()) {
                        Ordering::Equal => a.0.cmp(&b.0),
                        Ordering::Less => Ordering::Less,
                        Ordering::Greater => Ordering::Greater,
                    });
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
                let delta = match delta {
                    ScrollDelta::Lines { x, y } => x + y,
                    ScrollDelta::Pixels { x, y } => x + y,
                } as f64;
                if let Some(tx) = self.workspace_tx.as_mut() {
                    let _ = tx.try_send(WorkspaceEvent::Scroll(delta));
                }
            }
            Message::WorkspaceOverview => {
                let _ = ShellCommand::new("cosmic-workspaces").spawn();
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        if self.workspaces.is_empty() {
            return row![].padding(8).into();
        }
        let horizontal = matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        );
        let buttons = self.workspaces.iter().filter_map(|w| {
            let content = text(w.0.clone()).font(FONT_BOLD).size(16);

            let content = row!(
                content,
                vertical_space(Length::Fixed(
                    (self.core.applet.suggested_size().1 + 2 * self.core.applet.suggested_padding())
                        as f32
                ))
            )
            .align_items(cosmic::iced::Alignment::Center);

            let content = column!(
                content,
                horizontal_space(Length::Fixed(
                    (self.core.applet.suggested_size().0 + 2 * self.core.applet.suggested_padding())
                        as f32
                ))
            )
            .align_items(cosmic::iced::Alignment::Center);

            let btn = button(
                container(content)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center),
            )
            .padding(if horizontal {
                [0, self.core.applet.suggested_padding()]
            } else {
                [self.core.applet.suggested_padding(), 0]
            })
            .on_press(match w.1 {
                Some(zcosmic_workspace_handle_v1::State::Active) => Message::WorkspaceOverview,
                _ => Message::WorkspacePressed(w.2.clone()),
            })
            .padding(0);

            Some(
                btn.style(match w.1 {
                    Some(zcosmic_workspace_handle_v1::State::Active) => {
                        cosmic::theme::iced::Button::Primary
                    }
                    Some(zcosmic_workspace_handle_v1::State::Urgent) => {
                        let appearance = |theme: &Theme| {
                            let cosmic = theme.cosmic();
                            button::Appearance {
                                background: Some(Background::Color(
                                    cosmic.palette.neutral_3.into(),
                                )),
                                border: Border {
                                    radius: cosmic.radius_xl().into(),
                                    ..Default::default()
                                },
                                border_radius: theme.cosmic().radius_xl().into(),
                                text_color: theme.cosmic().destructive_button.base.into(),
                                ..button::Appearance::default()
                            }
                        };
                        cosmic::theme::iced::Button::Custom {
                            active: Box::new(appearance),
                            hover: Box::new(move |theme| button::Appearance {
                                background: Some(Background::Color(
                                    theme.current_container().component.hover.into(),
                                )),
                                border: Border {
                                    radius: theme.cosmic().radius_xl().into(),
                                    ..Default::default()
                                },
                                ..appearance(theme)
                            }),
                        }
                    }
                    None => {
                        let appearance = |theme: &Theme| {
                            let cosmic = theme.cosmic();
                            button::Appearance {
                                background: None,
                                border: Border {
                                    radius: cosmic.radius_xl().into(),
                                    ..Default::default()
                                },
                                border_radius: cosmic.radius_xl().into(),
                                text_color: theme.current_container().component.on.into(),
                                ..button::Appearance::default()
                            }
                        };
                        cosmic::theme::iced::Button::Custom {
                            active: Box::new(appearance),
                            hover: Box::new(move |theme| button::Appearance {
                                background: Some(Background::Color(
                                    theme.current_container().component.hover.into(),
                                )),
                                border: Border {
                                    radius: theme.cosmic().radius_xl().into(),
                                    ..Default::default()
                                },
                                ..appearance(theme)
                            }),
                        }
                    }
                    _ => return None,
                })
                .into(),
            )
        });
        let layout_section: Element<_> = match self.layout {
            Layout::Row => row(buttons).spacing(4).into(),
            Layout::Column => column(buttons).spacing(4).into(),
        };

        container(layout_section).padding(0).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            workspaces().map(Message::WorkspaceUpdate),
            event::listen_with(|e, _| match e {
                Mouse(mouse::Event::WheelScrolled { delta }) => Some(Message::WheelScrolled(delta)),
                _ => None,
            }),
        ])
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
