use cosmic::applet::{CosmicAppletHelper, APPLET_BUTTON_THEME};
use cosmic::iced::wayland::{
    popup::{destroy_popup, get_popup},
    SurfaceIdWrapper,
};
use cosmic::iced::{
    widget::{button, column, row, text, Row, Space},
    window, Alignment, Application, Color, Command, Length, Subscription,
};

use cosmic::iced_style::application::{self, Appearance};

use cosmic::theme::Svg;
use cosmic::widget::{divider, icon, toggler};
use cosmic::Renderer;
use cosmic::{Element, Theme};

use std::process;

pub fn main() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    Notifications::run(helper.window_settings())
}

#[derive(Default)]
struct Notifications {
    applet_helper: CosmicAppletHelper,
    theme: Theme,
    icon_name: String,
    popup: Option<window::Id>,
    id_ctr: u32,
    do_not_disturb: bool,
    notifications: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    DoNotDisturb(bool),
    Settings,
    Ignore,
}

impl Application for Notifications {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Notifications, Command<Message>) {
        (
            Notifications {
                icon_name: "notification-alert-symbolic".to_string(),
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Notifications")
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
                        None,
                        None,
                        None,
                    );
                    get_popup(popup_settings)
                }
            }
            Message::DoNotDisturb(b) => {
                self.do_not_disturb = b;
                Command::none()
            }
            Message::Settings => {
                let _ = process::Command::new("cosmic-settings notifications").spawn();
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
                let do_not_disturb =
                    row![
                        toggler(String::from("Do Not Disturb"), self.do_not_disturb, |b| {
                            Message::DoNotDisturb(b)
                        })
                        .width(Length::Fill)
                    ]
                    .padding([0, 24]);

                let settings =
                    row_button(vec!["Notification Settings...".into()]).on_press(Message::Settings);

                let notifications = if self.notifications.len() == 0 {
                    row![
                        Space::with_width(Length::Fill),
                        column![text_icon(&self.icon_name, 40), "No Notifications"]
                            .align_items(Alignment::Center),
                        Space::with_width(Length::Fill)
                    ]
                    .spacing(12)
                } else {
                    row![text("TODO: make app worky with notifications")]
                };

                let main_content = column![
                    divider::horizontal::light(),
                    notifications,
                    divider::horizontal::light()
                ]
                .padding([0, 24])
                .spacing(12);

                let content = column![]
                    .align_items(Alignment::Start)
                    .spacing(12)
                    .padding([12, 0])
                    .push(do_not_disturb)
                    .push(main_content)
                    .push(settings);

                self.applet_helper.popup_container(content).into()
            }
        }
    }
}

// todo put into libcosmic doing so will fix the row_button's boarder radius
fn row_button(
    mut content: Vec<Element<Message>>,
) -> cosmic::iced_native::widget::Button<Message, Renderer> {
    content.insert(0, Space::with_width(Length::Units(24)).into());
    content.push(Space::with_width(Length::Units(24)).into());

    button(
        Row::with_children(content)
            .spacing(4)
            .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Units(36))
    .style(APPLET_BUTTON_THEME)
}

fn text_icon(name: &str, size: u16) -> cosmic::widget::Icon {
    icon(name, size).style(Svg::Symbolic)
}
