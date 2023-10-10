use crate::dbus::{self, PowerDaemonProxy};
use crate::fl;
use crate::graphics::{get_current_graphics, set_graphics, Graphics};
use cosmic::app::command::message::cosmic;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced_runtime::core::alignment::Horizontal;
use cosmic::iced_runtime::core::Alignment;
use cosmic::iced_style::application;
use cosmic::theme::Button;
use cosmic::widget::{icon, Icon};
use cosmic::{
    applet::{button_theme, cosmic_panel_config::PanelAnchor},
    Command,
};
use cosmic::{
    iced::widget::{column, container, row, text},
    iced::{self, Length},
    iced_runtime::core::window,
    theme::Theme,
    widget::{button, divider},
    Element,
};
use zbus::Connection;

const ID: &str = "com.system76.CosmicAppletGraphics";

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
    core: cosmic::app::Core,
    popup: Option<window::Id>,
    graphics_mode: Option<GraphicsMode>,
    id_ctr: u128,
    dbus: Option<(Connection, PowerDaemonProxy<'static>)>,
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

impl cosmic::Application for Window {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (Self, iced::Command<cosmic::app::Message<Self::Message>>) {
        let window = Window {
            core,
            ..Default::default()
        };
        (
            window,
            iced::Command::perform(dbus::init(), |x| {
                cosmic::app::message::app(Message::DBusInit(x))
            }),
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
    ) -> iced::Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::SelectGraphicsMode(new) => {
                if let Some((_, proxy)) = self.dbus.as_ref() {
                    let prev = self
                        .graphics_mode
                        .map(|m| m.inner())
                        .unwrap_or_else(|| Graphics::Integrated);
                    self.graphics_mode = Some(GraphicsMode::SelectedGraphicsMode { prev, new });
                    return iced::Command::perform(
                        set_graphics(proxy.clone(), new),
                        move |success| {
                            cosmic::app::message::app(Message::AppliedGraphics(
                                success.ok().map(|_| new),
                            ))
                        },
                    );
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
                        commands.push(iced::Command::perform(
                            get_current_graphics(proxy.clone()),
                            |cur_graphics| Message::CurrentGraphics(cur_graphics.ok()),
                        ));
                    }
                    let popup_settings = self.core.applet.get_popup_settings(
                        window::Id(0),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    commands.push(get_popup(popup_settings));
                    return iced::Command::batch(commands).map(cosmic::app::message::app);
                }
            }
            Message::DBusInit(dbus) => {
                if dbus.is_none() {
                    eprintln!("Could not connect to com.system76.PowerDaemon. Exiting.");
                    std::process::exit(0);
                }
                self.dbus = dbus;
                return iced::Command::perform(
                    get_current_graphics(self.dbus.as_ref().unwrap().1.clone()),
                    |cur_graphics| {
                        Message::CurrentGraphics(match cur_graphics {
                            Ok(g) => Some(g),
                            Err(err) => {
                                eprintln!("{err:?}");
                                None
                            }
                        })
                    },
                )
                .map(cosmic::app::message::app);
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
                                return iced::Command::perform(
                                    set_graphics(proxy.clone(), prev),
                                    move |success| {
                                        Message::AppliedGraphics(success.ok().map(|_| new))
                                    },
                                )
                                .map(cosmic::app::message::app);
                            }
                        }
                        _ => {
                            return iced::Command::perform(
                                get_current_graphics(self.dbus.as_ref().unwrap().1.clone()),
                                |cur_graphics| {
                                    Message::CurrentGraphics(match cur_graphics {
                                        Ok(g) => Some(g),
                                        Err(err) => {
                                            tracing::error!("{:?}", err);
                                            None
                                        }
                                    })
                                },
                            )
                            .map(cosmic::app::message::app)
                        }
                    };
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        match self.core.applet.anchor {
            PanelAnchor::Left | PanelAnchor::Right => self
                .core
                .applet
                .icon_button(ID)
                .on_press(Message::TogglePopup)
                .into(),
            PanelAnchor::Top | PanelAnchor::Bottom => button(
                row![
                    Icon::from(
                        icon::from_name(ID)
                            .size(self.core.applet.suggested_size().0)
                            .symbolic(true)
                    )
                    .style(cosmic::theme::Svg::Custom(std::rc::Rc::new(
                        |theme| {
                            cosmic::iced_style::svg::Appearance {
                                color: Some(theme.cosmic().background.on.into()),
                            }
                        }
                    ))),
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
                .padding([0, self.core.applet.suggested_size().0 / 2])
                .align_items(Alignment::Center),
            )
            .style(Button::Text)
            .on_press(Message::TogglePopup)
            .padding(8)
            .width(Length::Shrink)
            .height(Length::Shrink)
            .into(),
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let symbolic = matches!(
            self.graphics_mode,
            Some(GraphicsMode::CurrentGraphicsMode(Graphics::Integrated))
                | Some(GraphicsMode::AppliedGraphicsMode(Graphics::Integrated))
                | Some(GraphicsMode::SelectedGraphicsMode {
                    new: Graphics::Integrated,
                    ..
                })
        );
        let content_list = vec![
            button(
                row![
                    column![
                        text(format!("{} {}", fl!("integrated"), fl!("graphics"))).size(14),
                        text(fl!("integrated-desc")).size(12)
                    ]
                    .width(Length::Fill),
                    icon::from_name(match self.graphics_mode {
                        Some(GraphicsMode::SelectedGraphicsMode {
                            new: Graphics::Integrated,
                            ..
                        }) => "process-working-symbolic",
                        _ => "emblem-ok-symbolic",
                    })
                    .size(12)
                    .symbolic(symbolic)
                    .prefer_svg(!symbolic)
                ]
                .align_items(Alignment::Center),
            )
            .style(button_theme())
            .padding([8, 24])
            .on_press(Message::SelectGraphicsMode(Graphics::Integrated))
            .width(Length::Fill)
            .into(),
            button(
                row![
                    column![text(format!("{} {}", fl!("nvidia"), fl!("graphics"))).size(14)]
                        .width(Length::Fill),
                    icon::from_name(match self.graphics_mode {
                        Some(GraphicsMode::SelectedGraphicsMode {
                            new: Graphics::Nvidia,
                            ..
                        }) => "process-working-symbolic",
                        _ => "emblem-ok-symbolic",
                    },)
                    .size(12)
                    .symbolic(symbolic)
                    .prefer_svg(!symbolic),
                ]
                .align_items(Alignment::Center),
            )
            .style(button_theme())
            .padding([8, 24])
            .on_press(Message::SelectGraphicsMode(Graphics::Nvidia))
            .width(Length::Fill)
            .into(),
            button(
                row![
                    column![
                        text(format!("{} {}", fl!("hybrid"), fl!("graphics"))).size(14),
                        text(fl!("hybrid-desc")).size(12)
                    ]
                    .width(Length::Fill),
                    icon::from_name(match self.graphics_mode {
                        Some(GraphicsMode::SelectedGraphicsMode {
                            new: Graphics::Hybrid,
                            ..
                        }) => "process-working-symbolic",
                        _ => "emblem-ok-symbolic",
                    },)
                    .size(12)
                    .symbolic(symbolic)
                    .prefer_svg(!symbolic),
                ]
                .align_items(Alignment::Center),
            )
            .style(button_theme())
            .padding([8, 24])
            .on_press(Message::SelectGraphicsMode(Graphics::Hybrid))
            .width(Length::Fill)
            .into(),
            button(
                row![
                    column![
                        text(format!("{} {}", fl!("compute"), fl!("graphics"))).size(14),
                        text(fl!("compute-desc")).size(12)
                    ]
                    .width(Length::Fill),
                    icon::from_name(match self.graphics_mode {
                        Some(GraphicsMode::SelectedGraphicsMode {
                            new: Graphics::Compute,
                            ..
                        }) => "process-working-symbolic",
                        _ => "emblem-ok-symbolic",
                    },)
                    .size(12)
                    .symbolic(symbolic)
                    .prefer_svg(!symbolic)
                ]
                .align_items(Alignment::Center),
            )
            .style(button_theme())
            .padding([8, 24])
            .on_press(Message::SelectGraphicsMode(Graphics::Compute))
            .width(Length::Fill)
            .into(),
        ];

        self.core
            .applet
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

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }
}
