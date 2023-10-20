use crate::dbus::{self, PowerDaemonProxy};
use crate::fl;
use crate::graphics::{get_current_graphics, set_graphics, Graphics};
use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced_runtime::core::alignment::Horizontal;
use cosmic::iced_runtime::core::Alignment;
use cosmic::iced_style::application;
use cosmic::widget::{horizontal_space, icon, Container, Icon};
use cosmic::{applet::cosmic_panel_config::PanelAnchor, Command};
use cosmic::{
    iced::widget::{column, row, text},
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
            .on_press(Message::TogglePopup)
            .padding(8)
            .width(Length::Shrink)
            .height(Length::Shrink)
            .style(cosmic::theme::Button::AppletIcon)
            .into(),
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let content_list = vec![
            menu_button(
                row![
                    column![
                        text(format!("{} {}", fl!("integrated"), fl!("graphics"))).size(14),
                        text(fl!("integrated-desc")).size(12)
                    ]
                    .width(Length::Fill),
                    button_icon(self.graphics_mode, Graphics::Integrated)
                ]
                .align_items(Alignment::Center),
            )
            .on_press(Message::SelectGraphicsMode(Graphics::Integrated))
            .into(),
            menu_button(
                row![
                    column![text(format!("{} {}", fl!("nvidia"), fl!("graphics"))).size(14)]
                        .width(Length::Fill),
                    button_icon(self.graphics_mode, Graphics::Nvidia)
                ]
                .align_items(Alignment::Center),
            )
            .on_press(Message::SelectGraphicsMode(Graphics::Nvidia))
            .into(),
            menu_button(
                row![
                    column![
                        text(format!("{} {}", fl!("hybrid"), fl!("graphics"))).size(14),
                        text(fl!("hybrid-desc")).size(12)
                    ]
                    .width(Length::Fill),
                    button_icon(self.graphics_mode, Graphics::Hybrid)
                ]
                .align_items(Alignment::Center),
            )
            .on_press(Message::SelectGraphicsMode(Graphics::Hybrid))
            .into(),
            menu_button(
                row![
                    column![
                        text(format!("{} {}", fl!("compute"), fl!("graphics"))).size(14),
                        text(fl!("compute-desc")).size(12)
                    ]
                    .width(Length::Fill),
                    button_icon(self.graphics_mode, Graphics::Compute)
                ]
                .align_items(Alignment::Center),
            )
            .on_press(Message::SelectGraphicsMode(Graphics::Compute))
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
                    padded_control(divider::horizontal::default()).into(),
                    column(content_list).into(),
                ])
                .padding([16, 0, 8, 0]),
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

fn button_icon<'a>(
    cur_mode: Option<GraphicsMode>,
    button_mode: Graphics,
) -> Container<'a, Message, cosmic::Renderer> {
    match cur_mode {
        Some(GraphicsMode::SelectedGraphicsMode { prev: _, new }) if new == button_mode => {
            cosmic::widget::container(
                icon::from_name("process-working-symbolic")
                    .size(12)
                    .symbolic(true)
                    .prefer_svg(true),
            )
        }
        Some(GraphicsMode::AppliedGraphicsMode(g) | GraphicsMode::CurrentGraphicsMode(g))
            if g == button_mode =>
        {
            cosmic::widget::container(
                icon::from_name("emblem-ok-symbolic")
                    .size(12)
                    .symbolic(true)
                    .prefer_svg(true),
            )
        }
        _ => cosmic::widget::container(horizontal_space(1.0)),
    }
}
