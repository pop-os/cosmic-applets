use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::{
    widget::{button, column, row, text, Row, Space},
    window, Alignment, Application, Color, Command, Length, Subscription,
};
use cosmic_applet::{applet_button_theme, CosmicAppletHelper};

use cosmic::iced_style::application::{self, Appearance};

use cosmic::iced_widget::Button;
use cosmic::theme::Svg;
use cosmic::widget::{divider, icon};
use cosmic::Renderer;
use cosmic::{Element, Theme};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};

use std::process;

pub fn main() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    Notifications::run(helper.window_settings())
}

static DO_NOT_DISTURB: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

#[derive(Default)]
struct Notifications {
    applet_helper: CosmicAppletHelper,
    theme: Theme,
    icon_name: String,
    popup: Option<window::Id>,
    id_ctr: u128,
    do_not_disturb: bool,
    notifications: Vec<Vec<String>>,
    timeline: Timeline,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    DoNotDisturb(chain::Toggler, bool),
    Settings,
    Ignore,
    Frame(Instant),
    Theme(Theme),
}

impl Application for Notifications {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Notifications, Command<Message>) {
        let applet_helper = CosmicAppletHelper::default();
        let theme = applet_helper.theme();
        (
            Notifications {
                applet_helper,
                theme,
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
        self.theme.clone()
    }

    fn close_requested(&self, _id: window::Id) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(Box::new(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        }))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            self.applet_helper.theme_subscription(0).map(Message::Theme),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
        ])
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Theme(t) => {
                self.theme = t;
                Command::none()
            }
            Message::Frame(now) => {
                self.timeline.now(now);
                Command::none()
            }
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    self.id_ctr += 1;
                    let new_id = window::Id(self.id_ctr);
                    self.popup.replace(new_id);

                    let popup_settings = self.applet_helper.get_popup_settings(
                        window::Id(0),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    get_popup(popup_settings)
                }
            }
            Message::DoNotDisturb(chain, b) => {
                self.timeline.set_chain(chain).start();
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

    fn view(&self, id: window::Id) -> Element<Message> {
        if id == window::Id(0) {
            self.applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into()
        } else {
            let do_not_disturb = row![anim!(
                DO_NOT_DISTURB,
                &self.timeline,
                String::from("Do Not Disturb"),
                self.do_not_disturb,
                Message::DoNotDisturb
            )
            .width(Length::Fill)]
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

// todo put into libcosmic doing so will fix the row_button's boarder radius
fn row_button(mut content: Vec<Element<Message>>) -> Button<Message, Renderer> {
    content.insert(0, Space::with_width(Length::Fixed(24.0)).into());
    content.push(Space::with_width(Length::Fixed(24.0)).into());

    button(
        Row::with_children(content)
            .spacing(4)
            .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(36.0))
    .style(applet_button_theme())
}

fn text_icon(name: &str, size: u16) -> cosmic::widget::Icon {
    icon(name, size).style(Svg::Symbolic)
}
