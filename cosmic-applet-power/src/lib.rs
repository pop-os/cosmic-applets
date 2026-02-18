// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    Element, Task, app,
    applet::{
        menu_button, padded_control,
        token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    },
    cctk::sctk::reexports::calloop,
    cosmic_theme::Spacing,
    iced::{
        self, Alignment, Length, Subscription,
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        widget::{self, column, row},
        window,
    },
    surface, theme,
    widget::{Space, button, divider, icon, text},
};
use std::sync::LazyLock;

use logind_zbus::{
    manager::ManagerProxy,
    session::{SessionClass, SessionProxy, SessionType},
    user::UserProxy,
};
use rustix::process::getuid;
use tokio::process;
use zbus::Connection;

pub mod cosmic_session;
mod localize;
pub mod session_manager;

use crate::{cosmic_session::CosmicSessionProxy, session_manager::SessionManagerProxy};

static SUBSURFACE_ID: LazyLock<cosmic::widget::Id> =
    LazyLock::new(|| cosmic::widget::Id::new("subsurface"));

pub fn run() -> cosmic::iced::Result {
    localize::localize();

    cosmic::applet::run::<Power>(())
}

struct Power {
    core: cosmic::app::Core,
    icon_name: String,
    popup: Option<window::Id>,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    subsurface_id: window::Id,
}

#[derive(Debug, Clone, Copy)]
enum PowerAction {
    Lock,
    LogOut,
    Suspend,
    Restart,
    Shutdown,
}

impl PowerAction {
    fn perform(self) -> iced::Task<cosmic::Action<Message>> {
        let msg = |m| cosmic::action::app(Message::Zbus(m));
        match self {
            PowerAction::Lock => iced::Task::perform(lock(), msg),
            PowerAction::LogOut => iced::Task::perform(log_out(), msg),
            PowerAction::Suspend => iced::Task::perform(suspend(), msg),
            PowerAction::Restart => iced::Task::perform(restart(), msg),
            PowerAction::Shutdown => iced::Task::perform(shutdown(), msg),
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Action(PowerAction),
    TogglePopup,
    OpenSettings,
    Zbus(Result<(), zbus::Error>),
    Closed(window::Id),
    Token(TokenUpdate),
    Surface(surface::Action),
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

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Self::Message>) {
        (
            Self {
                core,
                icon_name: "system-shutdown-symbolic".to_string(),
                subsurface_id: window::Id::unique(),
                token_tx: None,
                popup: Option::default(),
            },
            Task::none(),
        )
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::Closed(id))
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);

                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    return get_popup(popup_settings);
                }
            }
            Message::OpenSettings => {
                let exec = "cosmic-settings".to_string();
                if let Some(tx) = self.token_tx.as_ref() {
                    let _ = tx.send(TokenRequest {
                        app_id: Self::APP_ID.to_string(),
                        exec,
                    });
                } else {
                    tracing::error!("Wayland tx is None");
                }
            }
            Message::Action(action) => match action {
                PowerAction::LogOut => {
                    if let Err(err) = process::Command::new("cosmic-osd").arg("log-out").spawn() {
                        tracing::error!("Failed to spawn cosmic-osd. {err:?}");
                        return PowerAction::LogOut.perform();
                    }
                }
                PowerAction::Restart => {
                    if let Err(err) = process::Command::new("cosmic-osd").arg("restart").spawn() {
                        tracing::error!("Failed to spawn cosmic-osd. {err:?}");
                        return PowerAction::Restart.perform();
                    }
                }
                PowerAction::Shutdown => {
                    if let Err(err) = process::Command::new("cosmic-osd").arg("shutdown").spawn() {
                        tracing::error!("Failed to spawn cosmic-osd. {err:?}");
                        return PowerAction::Shutdown.perform();
                    }
                }
                a => return a.perform(),
            },
            Message::Zbus(result) => {
                if let Err(e) = result {
                    eprintln!("cosmic-applet-power ERROR: '{e}'");
                }
            }
            Message::Closed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Token(u) => match u {
                TokenUpdate::Init(tx) => {
                    self.token_tx = Some(tx);
                }
                TokenUpdate::Finished => {
                    self.token_tx = None;
                }
                TokenUpdate::ActivationToken { token, .. } => {
                    let mut cmd = std::process::Command::new("cosmic-settings");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
            },
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button(&self.icon_name)
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        let Spacing {
            space_xxs,
            space_s,
            space_m,
            ..
        } = theme::active().cosmic().spacing;

        if matches!(self.popup, Some(p) if p == id) {
            let settings = menu_button(text::body(fl!("settings"))).on_press(Message::OpenSettings);

            let session = column![
                menu_button(
                    row![
                        text_icon("system-lock-screen-symbolic", 24),
                        text::body(fl!("lock-screen")),
                        Space::with_width(Length::Fill),
                        text::body(fl!("lock-screen-shortcut")),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(space_xxs)
                )
                .on_press(Message::Action(PowerAction::Lock)),
                menu_button(
                    row![
                        text_icon("system-log-out-symbolic", 24),
                        text::body(fl!("log-out")),
                        Space::with_width(Length::Fill),
                        text::body(fl!("log-out-shortcut")),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(space_xxs)
                )
                .on_press(Message::Action(PowerAction::LogOut)),
            ];

            let power = row![
                power_buttons(
                    "system-suspend-symbolic",
                    Message::Action(PowerAction::Suspend)
                ),
                power_buttons(
                    "system-reboot-symbolic",
                    Message::Action(PowerAction::Restart)
                ),
                power_buttons(
                    "system-shutdown-symbolic",
                    Message::Action(PowerAction::Shutdown)
                )
            ]
            .spacing(space_m)
            .padding([0, space_m]);

            let content = column![
                settings,
                padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
                session,
                padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
                power
            ]
            .align_x(Alignment::Start)
            .padding([8, 0]);

            self.core.applet.popup_container(content).into()
        } else {
            //panic!("no view for window {}", id.0)
            widget::text("").into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        activation_token_subscription(0).map(Message::Token)
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}

fn power_buttons(name: &str, on_press: Message) -> button::Button<'_, Message> {
    button::custom(
        widget::container(text_icon(name, 40))
            .width(Length::Fill)
            .center(Length::Fill),
    )
    .on_press(on_press)
    .width(Length::Fill)
    .height(Length::Fixed(76.0))
    .class(theme::Button::Text)
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
    let mut locked_successfully = false;
    for (_, session_path) in sessions {
        let Ok(session) = SessionProxy::builder(&connection)
            .path(session_path)?
            .build()
            .await
        else {
            continue;
        };

        if session.class().await == Ok(SessionClass::User)
            && session.type_().await? != SessionType::TTY
            && session.lock().await.is_ok()
        {
            locked_successfully = true;
        }
    }

    if locked_successfully {
        Ok(())
    } else {
        Err(zbus::Error::Failure("locking session failed".to_string()))
    }
}

async fn log_out() -> zbus::Result<()> {
    let session_type = std::env::var("XDG_CURRENT_DESKTOP").ok();
    let connection = Connection::session().await?;
    if let Some("pop:GNOME") = session_type.as_ref().map(|s| s.trim()) {
        let manager_proxy = SessionManagerProxy::new(&connection).await?;
        manager_proxy.logout(0).await?;
    } else {
        // By default assume COSMIC
        let cosmic_session = CosmicSessionProxy::new(&connection).await?;
        cosmic_session.exit().await?;
    }
    Ok(())
}
