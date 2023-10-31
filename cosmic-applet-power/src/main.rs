use std::collections::HashMap;
use std::process;
use std::time::Duration;

use cosmic::applet::{menu_button, padded_control};
use cosmic::iced;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::event::wayland::{self, LayerEvent};
use cosmic::iced::event::PlatformSpecific;
use cosmic::iced::subscription::events_with;
use cosmic::iced::wayland::actions::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced_runtime::core::layout::Limits;
use cosmic::iced_sctk::commands::layer_surface::{
    destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity,
};
use cosmic::iced_widget::mouse_area;
use cosmic::widget::{button, divider, icon};
use cosmic::Renderer;

use cosmic::iced::Color;
use cosmic::iced::{
    widget::{self, column, container, row, space::Space, text},
    window, Alignment, Length, Subscription,
};
use cosmic::iced_style::application;
use cosmic::theme;
use cosmic::{app::Command, Element, Theme};

use logind_zbus::manager::ManagerProxy;
use logind_zbus::session::{SessionProxy, SessionType};
use logind_zbus::user::UserProxy;
use nix::unistd::getuid;
use tokio::time::sleep;
use zbus::Connection;

pub mod cosmic_session;
mod localize;
pub mod session_manager;

use crate::cosmic_session::CosmicSessionProxy;
use crate::session_manager::SessionManagerProxy;

pub fn main() -> cosmic::iced::Result {
    localize::localize();

    cosmic::applet::run::<Power>(false, ())
}

#[derive(Default)]
struct Power {
    core: cosmic::app::Core,
    icon_name: String,
    popup: Option<window::Id>,
    id_ctr: u128,
    action_to_confirm: Option<(window::Id, PowerAction)>,
}

#[derive(Debug, Clone, Copy)]
enum PowerAction {
    Lock,
    LogOut,
    Suspend,
    Restart,
    Shutdown,
}

#[derive(Debug, Clone)]
enum Message {
    Timeout(window::Id),
    Action(PowerAction),
    TogglePopup,
    Settings,
    Confirm,
    Cancel,
    Zbus(Result<(), zbus::Error>),
    Closed(window::Id),
}

impl cosmic::Application for Power {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.system76.CosmicAppletPower";

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn init(core: cosmic::app::Core, _flags: ()) -> (Power, Command<Message>) {
        (
            Power {
                core,
                icon_name: "system-shutdown-symbolic".to_string(),
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::Closed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        events_with(|e, _status| match e {
            cosmic::iced::Event::PlatformSpecific(PlatformSpecific::Wayland(
                wayland::Event::Layer(LayerEvent::Unfocused, ..),
            )) => Some(Message::Cancel),
            // cosmic::iced::Event::PlatformSpecific(PlatformSpecific::Wayland(
            //     wayland::Event::Seat(wayland::SeatEvent::Leave, _),
            // )) => Some(Message::Cancel),
            _ => None,
        })
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    self.id_ctr += 1;
                    let new_id = window::Id(self.id_ctr);
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        window::Id(0),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .min_width(100.0)
                        .min_height(100.0)
                        .max_height(400.0)
                        .max_width(500.0);
                    get_popup(popup_settings)
                }
            }
            Message::Settings => {
                let _ = process::Command::new("cosmic-settings").spawn();
                Command::none()
            }
            Message::Action(action) => {
                self.id_ctr += 1;
                let id = window::Id(self.id_ctr);
                self.action_to_confirm = Some((id, action));
                return Command::batch(vec![
                    iced::Command::perform(sleep(Duration::from_secs(60)), move |_| {
                        cosmic::app::message::app(Message::Timeout(id))
                    }),
                    get_layer_surface(SctkLayerSurfaceSettings {
                        id,
                        keyboard_interactivity: KeyboardInteractivity::None,
                        anchor: Anchor::all(),
                        namespace: "dialog".into(),
                        size: Some((None, None)),
                        size_limits: Limits::NONE.min_width(1.0).min_height(1.0),
                        ..Default::default()
                    }),
                ]);
            }
            Message::Zbus(result) => {
                if let Err(e) = result {
                    eprintln!("cosmic-applet-power ERROR: '{}'", e);
                }
                Command::none()
            }
            Message::Confirm => {
                if let Some((id, a)) = self.action_to_confirm.take() {
                    let msg = |m| cosmic::app::message::app(Message::Zbus(m));
                    Command::batch(vec![
                        destroy_layer_surface(id),
                        match a {
                            PowerAction::Lock => iced::Command::perform(lock(), msg),
                            PowerAction::LogOut => iced::Command::perform(log_out(), msg),
                            PowerAction::Suspend => iced::Command::perform(suspend(), msg),
                            PowerAction::Restart => iced::Command::perform(restart(), msg),
                            PowerAction::Shutdown => iced::Command::perform(shutdown(), msg),
                        },
                    ])
                } else {
                    Command::none()
                }
            }
            Message::Cancel => {
                if let Some((id, _)) = self.action_to_confirm.take() {
                    return destroy_layer_surface(id);
                }
                Command::none()
            }
            Message::Timeout(id) => {
                if let Some((surface_id, a)) = self.action_to_confirm {
                    if id == surface_id {
                        self.action_to_confirm = None;
                        let msg = |m: zbus::Result<()>| cosmic::app::message::app(Message::Zbus(m));
                        return Command::batch(vec![
                            destroy_layer_surface(id),
                            match a {
                                PowerAction::Lock => iced::Command::perform(lock(), msg),
                                PowerAction::LogOut => iced::Command::perform(log_out(), msg),
                                PowerAction::Suspend => iced::Command::perform(suspend(), msg),
                                PowerAction::Restart => iced::Command::perform(restart(), msg),
                                PowerAction::Shutdown => iced::Command::perform(shutdown(), msg),
                            },
                        ]);
                    }
                }
                Command::none()
            }
            Message::Closed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        self.core
            .applet
            .icon_button(&self.icon_name)
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, id: window::Id) -> Element<Message> {
        if matches!(self.popup, Some(p) if p == id) {
            let settings = menu_button(text(fl!("settings")).size(14)).on_press(Message::Settings);

            let session = column![
                menu_button(
                    row![
                        text_icon("system-lock-screen-symbolic", 24),
                        text(fl!("lock-screen")).size(14),
                        Space::with_width(Length::Fill),
                        text(fl!("lock-screen-shortcut")).size(14),
                    ]
                    .align_items(Alignment::Center)
                    .spacing(8)
                )
                .on_press(Message::Action(PowerAction::Lock)),
                menu_button(
                    row![
                        text_icon("system-log-out-symbolic", 24),
                        text(fl!("log-out")).size(14),
                        Space::with_width(Length::Fill),
                        text(fl!("log-out-shortcut")).size(14),
                    ]
                    .align_items(Alignment::Center)
                    .spacing(8)
                )
                .on_press(Message::Action(PowerAction::LogOut)),
            ];

            let power = row![
                power_buttons("system-lock-screen-symbolic", fl!("suspend"))
                    .on_press(Message::Action(PowerAction::Suspend)),
                power_buttons("system-restart-symbolic", fl!("restart"))
                    .on_press(Message::Action(PowerAction::Restart)),
                power_buttons("system-shutdown-symbolic", fl!("shutdown"))
                    .on_press(Message::Action(PowerAction::Shutdown)),
            ]
            .spacing(24)
            .padding([0, 24]);

            let content = column![
                settings,
                padded_control(divider::horizontal::default()),
                session,
                padded_control(divider::horizontal::default()),
                power
            ]
            .align_items(Alignment::Start)
            .padding([8, 0]);

            self.core.applet.popup_container(content).into()
        } else if matches!(self.action_to_confirm, Some((c_id, _)) if c_id == id) {
            let action = match self.action_to_confirm.as_ref().unwrap().1 {
                PowerAction::Lock => "lock-screen",
                PowerAction::LogOut => "log-out",
                PowerAction::Suspend => "suspend",
                PowerAction::Restart => "restart",
                PowerAction::Shutdown => "shutdown",
            };
            // TODO actual countdown
            let content = column![
                text(fl!(
                    "confirm-question",
                    HashMap::from_iter(vec![("action", action), ("countdown", "60")])
                ))
                .size(16),
                row![
                    button(text(fl!("confirm")).size(14))
                        .padding(8)
                        .style(theme::Button::Suggested)
                        .on_press(Message::Confirm),
                    button(text(fl!("cancel")).size(14))
                        .padding(8)
                        .style(theme::Button::Standard)
                        .on_press(Message::Cancel),
                ]
                .spacing(24)
            ]
            .align_items(Alignment::Center)
            .spacing(12)
            .padding(24);
            mouse_area(
                container(
                    container(content)
                        .style(cosmic::theme::Container::custom(|theme| {
                            container::Appearance {
                                icon_color: Some(theme.cosmic().background.on.into()),
                                text_color: Some(theme.cosmic().background.on.into()),
                                background: Some(
                                    Color::from(theme.cosmic().background.base).into(),
                                ),
                                border_radius: 12.0.into(),
                                border_width: 2.0,
                                border_color: theme.cosmic().bg_divider().into(),
                            }
                        }))
                        .width(Length::Shrink)
                        .height(Length::Shrink),
                )
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .on_press(Message::Cancel)
            .on_right_press(Message::Cancel)
            .on_middle_press(Message::Cancel)
            .into()
        } else {
            //panic!("no view for window {}", id.0)
            widget::text("").into()
        }
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}

fn power_buttons(name: &str, msg: String) -> cosmic::widget::Button<Message, Renderer> {
    cosmic::widget::button(
        column![text_icon(name, 40), text(msg).size(14)]
            .spacing(4)
            .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(76.0))
    .style(theme::Button::Text)
}

fn text_icon(name: &str, size: u16) -> cosmic::widget::Icon {
    icon::from_name(name).size(size).symbolic(true).icon()
}

// ### System helpers

async fn restart() -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    manager_proxy.reboot(true).await
}

async fn shutdown() -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    manager_proxy.power_off(true).await
}

async fn suspend() -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    manager_proxy.suspend(true).await
}

async fn lock() -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    // Get the session this current process is running in
    let our_uid = getuid().as_raw() as u32;
    let user_path = manager_proxy.get_user(our_uid).await?;
    let user = UserProxy::builder(&connection)
        .path(user_path)?
        .build()
        .await?;
    // Lock all non-TTY sessions of this user
    let sessions = user.sessions().await?;
    for (_, session_path) in sessions {
        let session = SessionProxy::builder(&connection)
            .path(session_path)?
            .build()
            .await?;
        if session.type_().await? != SessionType::TTY {
            session.lock().await?;
        }
    }
    Ok(())
}

async fn log_out() -> zbus::Result<()> {
    let session_type = std::env::var("XDG_CURRENT_DESKTOP").ok();
    let connection = Connection::session().await?;
    match session_type.as_ref().map(|s| s.trim()) {
        Some("pop:GNOME") => {
            let manager_proxy = SessionManagerProxy::new(&connection).await?;
            manager_proxy.logout(0).await?;
        }
        // By default assume COSMIC
        _ => {
            let cosmic_session = CosmicSessionProxy::new(&connection).await?;
            cosmic_session.exit().await?;
        }
    }
    Ok(())
}
