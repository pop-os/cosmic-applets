use iced::widget::Space;

use cosmic::applet::CosmicAppletHelper;
use cosmic::widget::{horizontal_rule, icon};
use cosmic::Renderer;

use cosmic::iced::{
    executor,
    widget::{button, column, row},
    window, Alignment, Application, Command, Length, Subscription,
};
use cosmic::iced_style::application::{self, Appearance};
use cosmic::iced_style::svg;
use cosmic::theme::{self, Svg};
use cosmic::{Element, Theme};

use iced_sctk::application::SurfaceIdWrapper;
use iced_sctk::commands::popup::{destroy_popup, get_popup};
use iced_sctk::widget::Row;
use iced_sctk::Color;

use logind_zbus::manager::ManagerProxy;
use zbus::Connection;

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
    Suspend,
    Restart,
    Shutdown,
    TogglePopup,
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

    fn close_requested(&self, _id: iced_sctk::application::SurfaceIdWrapper) -> Self::Message {
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
                        Some(200),
                        None,
                    );
                    get_popup(popup_settings)
                }
            }
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
                let settings = row_button(vec!["Settings...".into()]).on_press(Message::Ignore);

                let session = column![
                    row_button(vec![
                        text_icon("system-lock-screen-symbolic", 24).into(),
                        "Lock Screen".into(),
                        Space::with_width(Length::Fill).into(),
                        "Super + Escape".into(),
                    ])
                    .on_press(Message::Ignore),
                    row_button(vec![
                        text_icon("system-log-out-symbolic", 24).into(),
                        "Log Out".into(),
                        Space::with_width(Length::Fill).into(),
                        "Ctrl + Alt + Delete".into(),
                    ])
                    .on_press(Message::Ignore),
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
fn row_button(mut content: Vec<Element<Message>>) -> iced_sctk::widget::Button<Message, Renderer> {
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
) -> iced_sctk::widget::Button<'a, Message, Renderer> {
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
        fill: Some(theme.palette().text),
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
