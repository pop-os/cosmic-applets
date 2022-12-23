use cosmic::applet::CosmicAppletHelper;
use cosmic::iced::wayland::{
    popup::{destroy_popup, get_popup},
    SurfaceIdWrapper,
};
use cosmic::iced::{
    executor,
    widget::{button, column, horizontal_rule, row, Row, Space},
    window, Alignment, Application, Color, Command, Length, Subscription,
};
use cosmic::iced_style::application::{self, Appearance};
use cosmic::iced_style::svg;
use cosmic::theme::{self, Svg};
use cosmic::widget::icon;
use cosmic::Renderer;
use cosmic::{Element, Theme};

use logind_zbus::manager::ManagerProxy;
use logind_zbus::session::{SessionProxy, SessionType};
use logind_zbus::user::UserProxy;
use nix::unistd::getuid;
use std::process;
use zbus::Connection;

pub mod cosmic_session;
pub mod session_manager;

use crate::cosmic_session::CosmicSessionProxy;
use crate::session_manager::SessionManagerProxy;

pub fn main() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    Audio::run(helper.window_settings())
}

#[derive(Default)]
struct Audio {
    applet_helper: CosmicAppletHelper,
    icon_name: String,
    theme: Theme,
    popup: Option<window::Id>,
    id_ctr: u32,
}

#[derive(Debug, Clone)]
enum Message {
    Lock,
    LogOut,
    Suspend,
    Restart,
    Shutdown,
    TogglePopup,
    Settings,
    Ignore,
    Zbus(Result<(), zbus::Error>),
}

impl Application for Audio {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Audio, Command<Message>) {
        (
            Audio {
                icon_name: "system-shutdown-symbolic".to_string(),
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Power")
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn close_requested(&self, _id: SurfaceIdWrapper) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    self.id_ctr += 1;
                    let new_id = window::Id::new(self.id_ctr);
                    self.popup.replace(new_id);

                    let popup_settings = self.applet_helper.get_popup_settings(
                        window::Id::new(0),
                        new_id,
                        (400, 300),
                        None,
                        None,
                    );
                    get_popup(popup_settings)
                }
            }
            Message::Settings => {
                let _ = process::Command::new("cosmic-settings").spawn();
                Command::none()
            }
            Message::Lock => Command::perform(lock(), Message::Zbus),
            Message::LogOut => Command::perform(log_out(), Message::Zbus),
            Message::Suspend => Command::perform(suspend(), Message::Zbus),
            Message::Restart => Command::perform(restart(), Message::Zbus),
            Message::Shutdown => Command::perform(shutdown(), Message::Zbus),
            Message::Zbus(result) => {
                if let Err(e) = result {
                    eprintln!("cosmic-applet-power ERROR: '{}'", e);
                }
                Command::none()
            }
            Message::Ignore => Command::none(),
        }
    }

    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => self
                .applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into(),
            SurfaceIdWrapper::Popup(_) => {
                let settings = row_button(vec!["Settings...".into()]).on_press(Message::Settings);

                let session = column![
                    row_button(vec![
                        text_icon("system-lock-screen-symbolic", 24).into(),
                        "Lock Screen".into(),
                        Space::with_width(Length::Fill).into(),
                        "Super + Escape".into(),
                    ])
                    .on_press(Message::Lock),
                    row_button(vec![
                        text_icon("system-log-out-symbolic", 24).into(),
                        "Log Out".into(),
                        Space::with_width(Length::Fill).into(),
                        "Ctrl + Alt + Delete".into(),
                    ])
                    .on_press(Message::LogOut),
                ];

                let power = row![
                    power_buttons("system-lock-screen-symbolic", "Suspend")
                        .on_press(Message::Suspend),
                    power_buttons("system-restart-symbolic", "Restart").on_press(Message::Restart),
                    power_buttons("system-shutdown-symbolic", "Shutdown")
                        .on_press(Message::Shutdown),
                ]
                .spacing(24)
                .padding([0, 24]);

                let content = column![]
                    .align_items(Alignment::Start)
                    .spacing(12)
                    .padding([24, 0])
                    .push(settings)
                    .push(horizontal_rule(1))
                    .push(session)
                    .push(horizontal_rule(1))
                    .push(power);

                self.applet_helper.popup_container(content).into()
            }
        }
    }
}

// ### UI Helplers

// todo put into libcosmic doing so will fix the row_button's boarder radius
fn row_button(
    mut content: Vec<Element<Message>>,
) -> cosmic::iced::widget::Button<Message, Renderer> {
    content.insert(0, Space::with_width(Length::Units(24)).into());
    content.push(Space::with_width(Length::Units(24)).into());

    button(
        Row::with_children(content)
            .spacing(5)
            .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Units(35))
    .style(theme::Button::Text)
}

fn power_buttons<'a>(
    name: &'a str,
    text: &'a str,
) -> cosmic::iced::widget::Button<'a, Message, Renderer> {
    button(
        column![text_icon(name, 40), text]
            .spacing(5)
            .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Units(75))
    .style(theme::Button::Text)
}

fn text_icon(name: &str, size: u16) -> cosmic::widget::Icon {
    icon(name, size).style(Svg::Custom(|theme| svg::Appearance {
        color: Some(theme.palette().text),
    }))
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
        Some("pop:COSMIC") => {
            let cosmic_session = CosmicSessionProxy::new(&connection).await?;
            cosmic_session.exit().await?;
        }
        Some("pop:GNOME") => {
            let manager_proxy = SessionManagerProxy::new(&connection).await?;
            manager_proxy.logout(0).await?;
        }
        Some(desktop) => {
            eprintln!("unknown XDG_CURRENT_DESKTOP: {desktop}")
        }
        None => {}
    }
    Ok(())
}
