use crate::dbus::{self, PowerDaemonProxy};
use crate::fl;
use crate::graphics::{get_current_graphics, set_graphics, Graphics};
use cosmic::applet::CosmicAppletHelper;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::Color;
use cosmic::iced_runtime::core::alignment::Horizontal;
use cosmic::iced_runtime::core::Alignment;
use cosmic::iced_style::application::{self, Appearance};
use cosmic::theme::Button;
use cosmic::widget::icon;
use cosmic::{
    applet::{applet_button_theme, cosmic_panel_config::PanelAnchor},
    iced::widget::{column, container, row, text},
    iced::{self, Application, Command, Length},
    iced_runtime::core::window,
    theme::{Svg, Theme},
    widget::{button, divider},
    Element,
};
use zbus::Connection;

#[derive(Clone, Copy)]
enum GraphicsMode {
    AppliedGraphicsMode(Graphics),
    SelectedGraphicsMode { prev: Graphics, new: Graphics },
    CurrentGraphicsMode(Graphics),
}

impl GraphicsMode {
    fn inner(&self) -> Graphics {
        match self {
            GraphicsMode::SelectedGraphicsMode { new, .. } => *new,
            GraphicsMode::CurrentGraphicsMode(g) => *g,
            GraphicsMode::AppliedGraphicsMode(g) => *g,
        }
    }
}

#[derive(Default)]
pub struct Window {
    popup: Option<window::Id>,
    graphics_mode: Option<GraphicsMode>,
    id_ctr: u128,
    theme: Theme,
    dbus: Option<(Connection, PowerDaemonProxy<'static>)>,
    applet_helper: CosmicAppletHelper,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Message {
    CurrentGraphics(Option<Graphics>),
    AppliedGraphics(Option<Graphics>),
    DBusInit(Option<(Connection, PowerDaemonProxy<'static>)>),
    SelectGraphicsMode(Graphics),
    TogglePopup,
    PopupClosed(window::Id),
}

impl Application for Window {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    type Theme = Theme;

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let window = Window::default();
        (window, Command::perform(dbus::init(), Message::DBusInit))
    }

    fn title(&self) -> String {
        String::from("Cosmic Graphics Applet")
    }

    fn update(&mut self, message: Message) -> iced::Command<Self::Message> {
        match message {
            Message::SelectGraphicsMode(new) => {
                if let Some((_, proxy)) = self.dbus.as_ref() {
                    let prev = self
                        .graphics_mode
                        .map(|m| m.inner())
                        .unwrap_or_else(|| Graphics::Integrated);
                    self.graphics_mode = Some(GraphicsMode::SelectedGraphicsMode { prev, new });
                    return Command::perform(set_graphics(proxy.clone(), new), move |success| {
                        Message::AppliedGraphics(success.ok().map(|_| new))
                    });
                }
            }
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    self.id_ctr += 1;
                    let new_id = window::Id(self.id_ctr);
                    self.popup.replace(new_id);
                    let mut commands = Vec::new();
                    if let Some((_, proxy)) = self.dbus.as_ref() {
                        commands.push(Command::perform(
                            get_current_graphics(proxy.clone()),
                            |cur_graphics| Message::CurrentGraphics(cur_graphics.ok()),
                        ));
                    }
                    let popup_settings = self.applet_helper.get_popup_settings(
                        window::Id(0),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    commands.push(get_popup(popup_settings));
                    return Command::batch(commands);
                }
            }
            Message::DBusInit(dbus) => {
                self.dbus = dbus;
                return Command::perform(
                    get_current_graphics(self.dbus.as_ref().unwrap().1.clone()),
                    |cur_graphics| {
                        Message::CurrentGraphics(match cur_graphics {
                            Ok(g) => Some(g),
                            Err(err) => {
                                eprintln!("{err}");
                                None
                            }
                        })
                    },
                );
            }
            Message::CurrentGraphics(g) => {
                if let Some(g) = g {
                    self.graphics_mode = Some(match self.graphics_mode.take() {
                        Some(GraphicsMode::CurrentGraphicsMode(_)) | None => {
                            GraphicsMode::CurrentGraphicsMode(g)
                        }
                        Some(g) => g,
                    });
                }
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::AppliedGraphics(g) => {
                if let Some(g) = g {
                    self.graphics_mode = Some(GraphicsMode::AppliedGraphicsMode(g));
                } else {
                    // Reset graphics
                    match self.graphics_mode {
                        Some(GraphicsMode::SelectedGraphicsMode { prev, new }) => {
                            // TODO send notification with error?
                            self.graphics_mode = Some(GraphicsMode::AppliedGraphicsMode(prev));
                            // Reset to prev after failing
                            // https://github.com/pop-os/system76-power/issues/387
                            if let Some((_, proxy)) = self.dbus.as_ref() {
                                return Command::perform(
                                    set_graphics(proxy.clone(), prev),
                                    move |success| {
                                        Message::AppliedGraphics(success.ok().map(|_| new))
                                    },
                                );
                            }
                        }
                        _ => {
                            return Command::perform(
                                get_current_graphics(self.dbus.as_ref().unwrap().1.clone()),
                                |cur_graphics| {
                                    Message::CurrentGraphics(match cur_graphics {
                                        Ok(g) => Some(g),
                                        Err(err) => {
                                            dbg!(err);
                                            None
                                        }
                                    })
                                },
                            )
                        }
                    };
                }
            }
        }
        Command::none()
    }

    fn view(&self, id: window::Id) -> Element<Message> {
        if id == window::Id(0) {
            match self.applet_helper.anchor {
                PanelAnchor::Left | PanelAnchor::Right => self
                    .applet_helper
                    .icon_button("input-gaming-symbolic")
                    .on_press(Message::TogglePopup)
                    .style(Button::Text)
                    .into(),
                PanelAnchor::Top | PanelAnchor::Bottom => button(Button::Text)
                    .custom(vec![row![
                        icon(
                            "input-gaming-symbolic",
                            self.applet_helper.suggested_size().0,
                        )
                        .style(Svg::Symbolic),
                        text(match self.graphics_mode.map(|g| g.inner()) {
                            Some(Graphics::Integrated) => fl!("integrated"),
                            Some(Graphics::Nvidia) => fl!("nvidia"),
                            Some(Graphics::Compute) => fl!("compute"),
                            Some(Graphics::Hybrid) => fl!("hybrid"),
                            None => "".into(),
                        })
                        .size(14)
                    ]
                    .spacing(8)
                    .padding([0, self.applet_helper.suggested_size().0 / 2])
                    .align_items(Alignment::Center)
                    .into()])
                    .style(Button::Text)
                    .on_press(Message::TogglePopup)
                    .padding(8)
                    .width(Length::Shrink)
                    .height(Length::Shrink)
                    .into(),
            }
        } else {
            let content_list = vec![
                button(applet_button_theme())
                    .custom(vec![row![
                        column![
                            text(format!("{} {}", fl!("integrated"), fl!("graphics"))).size(14),
                            text(fl!("integrated-desc")).size(12)
                        ]
                        .width(Length::Fill),
                        icon(
                            match self.graphics_mode {
                                Some(GraphicsMode::SelectedGraphicsMode {
                                    new: Graphics::Integrated,
                                    ..
                                }) => "process-working-symbolic",
                                _ => "emblem-ok-symbolic",
                            },
                            12
                        )
                        .size(12)
                        .style(match self.graphics_mode {
                            Some(GraphicsMode::CurrentGraphicsMode(Graphics::Integrated)) =>
                                Svg::SymbolicActive,
                            Some(GraphicsMode::AppliedGraphicsMode(Graphics::Integrated)) =>
                                Svg::SymbolicActive,
                            Some(GraphicsMode::SelectedGraphicsMode {
                                new: Graphics::Integrated,
                                ..
                            }) => Svg::Symbolic,
                            _ => Svg::Default,
                        },),
                    ]
                    .align_items(Alignment::Center)
                    .into()])
                    .padding([8, 24])
                    .on_press(Message::SelectGraphicsMode(Graphics::Integrated))
                    .width(Length::Fill)
                    .into(),
                button(applet_button_theme())
                    .custom(vec![row![
                        column![text(format!("{} {}", fl!("nvidia"), fl!("graphics"))).size(14),]
                            .width(Length::Fill),
                        icon(
                            match self.graphics_mode {
                                Some(GraphicsMode::SelectedGraphicsMode {
                                    new: Graphics::Nvidia,
                                    ..
                                }) => "process-working-symbolic",
                                _ => "emblem-ok-symbolic",
                            },
                            12
                        )
                        .size(12)
                        .style(match self.graphics_mode {
                            Some(GraphicsMode::CurrentGraphicsMode(Graphics::Nvidia)) =>
                                Svg::SymbolicActive,
                            Some(GraphicsMode::AppliedGraphicsMode(Graphics::Nvidia)) =>
                                Svg::SymbolicActive,
                            Some(GraphicsMode::SelectedGraphicsMode {
                                new: Graphics::Nvidia,
                                ..
                            }) => Svg::Symbolic,
                            _ => Svg::Default,
                        }),
                    ]
                    .align_items(Alignment::Center)
                    .into()])
                    .padding([8, 24])
                    .on_press(Message::SelectGraphicsMode(Graphics::Nvidia))
                    .width(Length::Fill)
                    .into(),
                button(applet_button_theme())
                    .custom(vec![row![
                        column![
                            text(format!("{} {}", fl!("hybrid"), fl!("graphics"))).size(14),
                            text(fl!("hybrid-desc")).size(12)
                        ]
                        .width(Length::Fill),
                        icon(
                            match self.graphics_mode {
                                Some(GraphicsMode::SelectedGraphicsMode {
                                    new: Graphics::Hybrid,
                                    ..
                                }) => "process-working-symbolic",
                                _ => "emblem-ok-symbolic",
                            },
                            12
                        )
                        .size(12)
                        .style(match self.graphics_mode {
                            Some(GraphicsMode::CurrentGraphicsMode(Graphics::Hybrid)) =>
                                Svg::SymbolicActive,
                            Some(GraphicsMode::AppliedGraphicsMode(Graphics::Hybrid)) =>
                                Svg::SymbolicActive,
                            Some(GraphicsMode::SelectedGraphicsMode {
                                new: Graphics::Hybrid,
                                ..
                            }) => Svg::Symbolic,
                            _ => Svg::Default,
                        })
                    ]
                    .align_items(Alignment::Center)
                    .into()])
                    .padding([8, 24])
                    .on_press(Message::SelectGraphicsMode(Graphics::Hybrid))
                    .width(Length::Fill)
                    .into(),
                button(applet_button_theme())
                    .custom(vec![row![
                        column![
                            text(format!("{} {}", fl!("compute"), fl!("graphics"))).size(14),
                            text(fl!("compute-desc")).size(12)
                        ]
                        .width(Length::Fill),
                        icon(
                            match self.graphics_mode {
                                Some(GraphicsMode::SelectedGraphicsMode {
                                    new: Graphics::Compute,
                                    ..
                                }) => "process-working-symbolic",
                                _ => "emblem-ok-symbolic",
                            },
                            12
                        )
                        .size(12)
                        .style(match self.graphics_mode {
                            Some(GraphicsMode::CurrentGraphicsMode(Graphics::Compute)) =>
                                Svg::SymbolicActive,
                            Some(GraphicsMode::AppliedGraphicsMode(Graphics::Compute)) =>
                                Svg::SymbolicActive,
                            Some(GraphicsMode::SelectedGraphicsMode {
                                new: Graphics::Compute,
                                ..
                            }) => Svg::Symbolic,
                            _ => Svg::Default,
                        }),
                    ]
                    .align_items(Alignment::Center)
                    .into()])
                    .padding([8, 24])
                    .on_press(Message::SelectGraphicsMode(Graphics::Compute))
                    .width(Length::Fill)
                    .into(),
            ];

            self.applet_helper
                .popup_container(
                    column(vec![
                        text(fl!("graphics-mode"))
                            .width(Length::Fill)
                            .horizontal_alignment(Horizontal::Center)
                            .size(14)
                            .into(),
                        container(divider::horizontal::light())
                            .padding([0, 12])
                            .width(Length::Fill)
                            .into(),
                        column(content_list).into(),
                    ])
                    .padding([8, 0])
                    .spacing(12),
                )
                .into()
        }
    }

    fn close_requested(&self, id: window::Id) -> Self::Message {
        if id != window::Id(0) {
            Message::PopupClosed(id)
        } else {
            unimplemented!();
        }
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(Box::new(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().background.on.into(),
        }))
    }

    fn should_exit(&self) -> bool {
        false
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }
}
