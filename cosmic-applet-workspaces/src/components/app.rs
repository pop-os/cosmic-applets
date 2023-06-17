use calloop::channel::SyncSender;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::mouse::{self, ScrollDelta};
use cosmic::iced::wayland::actions::window::SctkWindowSettings;
use cosmic::iced::wayland::{window::resize_window, InitialSurface};
use cosmic::iced::widget::{column, container, row, text};
use cosmic::iced::Color;
use cosmic::iced::{
    subscription, widget::button, window, Application, Command, Event::Mouse, Length, Settings,
    Subscription,
};
use cosmic::iced_style::application::{self, Appearance};
use cosmic::theme::Button;
use cosmic::{Element, Theme};
use cosmic_applet::cosmic_panel_config::PanelAnchor;
use cosmic_applet::CosmicAppletHelper;
use cosmic_protocols::workspace::v1::client::zcosmic_workspace_handle_v1;
use std::cmp::Ordering;
use wayland_backend::client::ObjectId;

use crate::config;
use crate::wayland::{WorkspaceEvent, WorkspaceList};
use crate::wayland_subscription::{workspaces, WorkspacesUpdate};

pub fn run() -> cosmic::iced::Result {
    let settings = Settings {
        initial_surface: InitialSurface::XdgWindow(SctkWindowSettings {
            size: (32, 32),
            autosize: true,
            resizable: None,
            ..Default::default()
        }),
        ..Default::default()
    };
    IcedWorkspacesApplet::run(settings)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Row,
    Column,
}

struct IcedWorkspacesApplet {
    theme: Theme,
    workspaces: WorkspaceList,
    workspace_tx: Option<SyncSender<WorkspaceEvent>>,
    layout: Layout,
    helper: CosmicAppletHelper,
}

#[derive(Debug, Clone)]
enum Message {
    WorkspaceUpdate(WorkspacesUpdate),
    WorkspacePressed(ObjectId),
    WheelScrolled(ScrollDelta),
    Theme(Theme),
    Errored,
}

impl Application for IcedWorkspacesApplet {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let applet_helper = CosmicAppletHelper::default();
        (
            IcedWorkspacesApplet {
                layout: match &applet_helper.anchor {
                    PanelAnchor::Left | PanelAnchor::Right => Layout::Column,
                    PanelAnchor::Top | PanelAnchor::Bottom => Layout::Row,
                },
                theme: applet_helper.theme(),
                workspaces: Vec::new(),
                workspace_tx: Default::default(),
                helper: Default::default(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        config::APP_ID.to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
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
            Message::Errored => {}
            Message::Theme(t) => self.theme = t,
        }
        Command::none()
    }

    fn view(&self, _id: window::Id) -> Element<Message> {
        if self.workspaces.is_empty() {
            return row![].padding(8).into();
        }
        let buttons = self
            .workspaces
            .iter()
            .filter_map(|w| {
                let btn = button(
                    text(w.0.clone())
                        .size(14)
                        .horizontal_alignment(Horizontal::Center)
                        .vertical_alignment(Vertical::Center)
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .width(Length::Fixed(self.helper.suggested_size().0 as f32 + 16.0))
                .height(Length::Fixed(self.helper.suggested_size().0 as f32 + 16.0))
                .on_press(Message::WorkspacePressed(w.2.clone()))
                .padding(0);
                Some(
                    btn.style(match w.1 {
                        Some(zcosmic_workspace_handle_v1::State::Active) => Button::Primary,
                        Some(zcosmic_workspace_handle_v1::State::Urgent) => Button::Destructive,
                        None => Button::Secondary,
                        _ => return None,
                    })
                    .into(),
                )
            })
            .collect();
        let layout_section: Element<_> = match self.layout {
            Layout::Row => row(buttons)
                .width(Length::Shrink)
                .height(Length::Shrink)
                .padding(0)
                .into(),
            Layout::Column => column(buttons)
                .width(Length::Shrink)
                .height(Length::Shrink)
                .padding(0)
                .into(),
        };

        container(layout_section)
            .width(Length::Shrink)
            .height(Length::Shrink)
            .padding(0)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(
            vec![
                self.helper.theme_subscription(0).map(Message::Theme),
                workspaces(0).map(|e| Message::WorkspaceUpdate(e.1)),
                subscription::events_with(|e, _| match e {
                    Mouse(mouse::Event::WheelScrolled { delta }) => {
                        Some(Message::WheelScrolled(delta))
                    }
                    _ => None,
                }),
            ]
            .into_iter(),
        )
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn close_requested(&self, _id: window::Id) -> Self::Message {
        unimplemented!()
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(Box::new(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        }))
    }
}
