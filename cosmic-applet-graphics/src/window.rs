use crate::dbus::{self, PowerDaemonProxy};
use crate::graphics::{get_current_graphics, set_graphics, Graphics};
use cosmic::applet::{CosmicAppletHelper};
use cosmic::iced_style::application::{self, Appearance};
use cosmic::theme::Button;
use cosmic::{
    iced::widget::{column, radio, text},
    iced::{self, Application, Command, Length},
    iced_native::window,
    theme::Theme,
    widget::{horizontal_rule},
    Element,
};
use cosmic_panel_config::{PanelAnchor, PanelSize};
use iced_sctk::alignment::Horizontal;
use iced_sctk::application::SurfaceIdWrapper;
use iced_sctk::commands::popup::{destroy_popup, get_popup};
use iced_sctk::Color;
use zbus::Connection;

#[derive(Clone, Copy)]
enum State {
    SelectGraphicsMode(bool),
    SettingGraphicsMode(Graphics),
}

#[derive(Clone, Copy)]
enum GraphicsMode {
    SelectedGraphicsMode(Graphics),
    CurrentGraphicsMode(Graphics),
}

impl GraphicsMode {
    fn inner(&self) -> Graphics {
        match self {
            GraphicsMode::SelectedGraphicsMode(g) => *g,
            GraphicsMode::CurrentGraphicsMode(g) => *g,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::SelectGraphicsMode(false)
    }
}

#[derive(Default)]
pub struct Window {
    popup: Option<window::Id>,
    graphics_mode: Option<GraphicsMode>,
    id_ctr: u32,
    icon_size: u16,
    anchor: PanelAnchor,
    theme: Theme,
    dbus: Option<(Connection, PowerDaemonProxy<'static>)>,
    state: State,
    applet_helper: CosmicAppletHelper,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Message {
    CurrentGraphics(Option<Graphics>),
    SelectedGraphicsMode(Option<Graphics>),
    DBusInit(Option<(Connection, PowerDaemonProxy<'static>)>),
    SelectGraphicsMode(Graphics),
    TogglePopup,
    PopupClosed(window::Id),
}

impl Application for Window {
    type Executor = iced::executor::Default;
    type Flags = ();
    type Message = Message;
    type Theme = Theme;

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let mut window = Window::default();
        let pixels = std::env::var("COSMIC_PANEL_SIZE")
            .ok()
            .and_then(|size| match size.parse::<PanelSize>() {
                Ok(PanelSize::XL) => Some(64),
                Ok(PanelSize::L) => Some(36),
                Ok(PanelSize::M) => Some(24),
                Ok(PanelSize::S) => Some(16),
                Ok(PanelSize::XS) => Some(12),
                Err(_) => Some(12),
            })
            .unwrap_or(16);
        window.icon_size = pixels;
        window.anchor = std::env::var("COSMIC_PANEL_ANCHOR")
            .ok()
            .map(|size| match size.parse::<PanelAnchor>() {
                Ok(p) => p,
                Err(_) => PanelAnchor::Top,
            })
            .unwrap_or(PanelAnchor::Top);
        (
            window,
            Command::perform(dbus::init(), |dbus_init| Message::DBusInit(dbus_init)),
        )
    }

    fn title(&self) -> String {
        String::from("Cosmic Graphics Applet")
    }

    fn update(&mut self, message: Message) -> iced::Command<Self::Message> {
        match message {
            Message::SelectGraphicsMode(new_graphics_mode) => {
                if let Some((_, proxy)) = self.dbus.as_ref() {
                    self.state = State::SettingGraphicsMode(new_graphics_mode);
                    return Command::perform(
                        set_graphics(proxy.clone(), new_graphics_mode),
                        move |success| {
                            Message::SelectedGraphicsMode(success.ok().map(|_| new_graphics_mode))
                        },
                    );
                }
            }
            Message::SelectedGraphicsMode(g) => {
                if let Some(g) = g {
                    self.graphics_mode
                        .replace(GraphicsMode::SelectedGraphicsMode(g));
                    self.state = State::SelectGraphicsMode(true);
                }
            }
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    self.id_ctr += 1;
                    let new_id = window::Id::new(self.id_ctr);
                    self.popup.replace(new_id);
                    let mut commands = Vec::new();
                    if let Some((_, proxy)) = self.dbus.as_ref() {
                        commands.push(Command::perform(
                            get_current_graphics(proxy.clone()),
                            |cur_graphics| Message::CurrentGraphics(cur_graphics.ok()),
                        ));
                    }
                    let popup_settings =
                        self.applet_helper.get_popup_settings(window::Id::new(0), new_id, (200, 240), None, None);
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
                                dbg!(err);
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
        }
        Command::none()
    }

    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => self.applet_helper.icon_button("input-gaming-symbolic")
                .on_press(Message::TogglePopup)
                .style(Button::Text)
                .into(),
            SurfaceIdWrapper::Popup(_) => {
                let content = match self.state {
                    State::SelectGraphicsMode(pending_restart) => {
                        let mut content_list = vec![
                            radio(
                                "Integrated Graphics",
                                Graphics::Integrated,
                                self.graphics_mode.map(|g| g.inner()),
                                |g| Message::SelectGraphicsMode(g),
                            )
                            .into(),
                            radio(
                                "Nvidia Graphics",
                                Graphics::Nvidia,
                                self.graphics_mode.map(|g| g.inner()),
                                |g| Message::SelectGraphicsMode(g),
                            )
                            .into(),
                            radio(
                                "Hybrid Graphics",
                                Graphics::Hybrid,
                                self.graphics_mode.map(|g| g.inner()),
                                |g| Message::SelectGraphicsMode(g),
                            )
                            .into(),
                            radio(
                                "Compute Graphics",
                                Graphics::Compute,
                                self.graphics_mode.map(|g| g.inner()),
                                |g| Message::SelectGraphicsMode(g),
                            )
                            .into(),
                        ];
                        if pending_restart {
                            content_list.insert(
                                0,
                                text("Restart to apply changes")
                                    .width(Length::Fill)
                                    .horizontal_alignment(Horizontal::Center)
                                    .size(16)
                                    .into(),
                            )
                        }
                        column(content_list).padding([8, 0]).spacing(8).into()
                    }
                    State::SettingGraphicsMode(graphics) => {
                        let graphics_str = match graphics {
                            Graphics::Integrated => "integrated",
                            Graphics::Hybrid => "hybrid",
                            Graphics::Nvidia => "nvidia",
                            Graphics::Compute => "compute",
                        };
                        column(vec![text(format!(
                            "Setting graphics mode to {graphics_str}..."
                        ))
                        .width(Length::Fill)
                        .horizontal_alignment(Horizontal::Center)
                        .into()])
                        .into()
                    }
                };
                self.applet_helper.popup_container(
                    column(vec![
                        text("Graphics Mode")
                            .width(Length::Fill)
                            .horizontal_alignment(Horizontal::Center)
                            .size(24)
                            .into(),
                        horizontal_rule(1).into(),
                        content,
                    ])
                    .padding(4)
                    .spacing(4),
                )
                .into()
            }
        }
    }

    fn close_requested(&self, id: SurfaceIdWrapper) -> Self::Message {
        match id {
            SurfaceIdWrapper::LayerSurface(_) | SurfaceIdWrapper::Window(_) => unimplemented!(),
            SurfaceIdWrapper::Popup(id) => Message::PopupClosed(id),
        }
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }

    fn should_exit(&self) -> bool {
        false
    }

    fn theme(&self) -> Theme {
        self.theme
    }
}
