mod localize;
mod subscriptions;

use cosmic::cosmic_config::{config_subscription, Config, CosmicConfigEntry};
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::Limits;
use cosmic::iced::{
    widget::{button, column, row, text, Row, Space},
    window, Alignment, Application, Color, Command, Length, Subscription,
};
use cosmic::iced_core::alignment::Horizontal;
use cosmic::iced_core::image;
use cosmic::iced_widget::button::StyleSheet;
use cosmic_applet::{applet_button_theme, CosmicAppletHelper};

use cosmic::iced_style::application::{self, Appearance};

use cosmic::iced_widget::{scrollable, Column};
use cosmic::theme::{Button, Svg};
use cosmic::widget::{container, divider, icon};
use cosmic::Renderer;
use cosmic::{Element, Theme};
use cosmic_notifications_config::NotificationsConfig;
use cosmic_notifications_util::{AppletEvent, Notification};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};
use std::borrow::Cow;
use std::collections::HashMap;
use std::process;
use tokio::sync::mpsc::Sender;
use tracing::info;

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt::init();
    // Prepare i18n
    localize::localize();

    info!("Notifications applet");

    let helper = CosmicAppletHelper::default();
    Notifications::run(helper.window_settings())
}

static DO_NOT_DISTURB: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);
#[derive(Default)]
struct Notifications {
    applet_helper: CosmicAppletHelper,
    theme: Theme,
    config: NotificationsConfig,
    config_helper: Option<Config>,
    icon_name: String,
    popup: Option<window::Id>,
    id_ctr: u128,
    notifications: Vec<Notification>,
    timeline: Timeline,
    dbus_sender: Option<Sender<subscriptions::dbus::Input>>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    DoNotDisturb(chain::Toggler, bool),
    Settings,
    Ignore,
    Frame(Instant),
    Theme(Theme),
    NotificationEvent(AppletEvent),
    Config(NotificationsConfig),
    DbusEvent(subscriptions::dbus::Output),
    Dismissed(u32),
    ClearAll,
}

impl Application for Notifications {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Notifications, Command<Message>) {
        let applet_helper = CosmicAppletHelper::default();
        let theme = applet_helper.theme();
        let helper = Config::new(
            cosmic_notifications_config::ID,
            NotificationsConfig::version(),
        )
        .ok();

        let config: NotificationsConfig = helper
            .as_ref()
            .map(|helper| {
                NotificationsConfig::get_entry(helper).unwrap_or_else(|(errors, config)| {
                    for err in errors {
                        tracing::error!("{:?}", err);
                    }
                    config
                })
            })
            .unwrap_or_default();
        (
            Notifications {
                applet_helper,
                theme,
                icon_name: "notification-alert-symbolic".to_string(),
                config_helper: helper,
                config,
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
            config_subscription::<u64, NotificationsConfig>(
                0,
                cosmic_notifications_config::ID.into(),
                NotificationsConfig::version(),
            )
            .map(|(_, res)| match res {
                Ok(config) => Message::Config(config),
                Err((errors, config)) => {
                    for err in errors {
                        tracing::error!("{:?}", err);
                    }
                    Message::Config(config)
                }
            }),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
            subscriptions::dbus::proxy().map(Message::DbusEvent),
            subscriptions::notifications::notifications().map(Message::NotificationEvent),
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

                    let mut popup_settings = self.applet_helper.get_popup_settings(
                        window::Id(0),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .min_width(1.0)
                        .max_width(300.0)
                        .min_height(100.0)
                        .max_height(900.0);
                    get_popup(popup_settings)
                }
            }
            Message::DoNotDisturb(chain, b) => {
                self.timeline.set_chain(chain).start();
                self.config.do_not_disturb = b;
                if let Some(helper) = &self.config_helper {
                    if let Err(err) = self.config.write_entry(helper) {
                        tracing::error!("{:?}", err);
                    }
                }
                Command::none()
            }
            Message::Settings => {
                let _ = process::Command::new("cosmic-settings notifications").spawn();
                Command::none()
            }
            Message::NotificationEvent(e) => {
                match e {
                    AppletEvent::Notification(n) => {
                        self.notifications.push(n);
                    }
                    AppletEvent::Replace(n) => {
                        if let Some(old) = self.notifications.iter_mut().find(|n| n.id == n.id) {
                            *old = n;
                        }
                    }
                    AppletEvent::Closed(id) => {
                        self.notifications.retain(|n| n.id != id);
                    }
                }
                Command::none()
            }
            Message::Ignore => Command::none(),
            Message::Config(config) => {
                self.config = config;
                Command::none()
            }
            Message::Dismissed(id) => {
                self.notifications.retain(|n| n.id != id);
                if let Some(tx) = &self.dbus_sender {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Err(err) = tx.send(subscriptions::dbus::Input::Dismiss(id)).await {
                            tracing::error!("{:?}", err);
                        }
                    });
                }
                Command::none()
            }
            Message::DbusEvent(e) => match e {
                subscriptions::dbus::Output::Ready(tx) => {
                    self.dbus_sender.replace(tx);
                    Command::none()
                }
            },
            Message::ClearAll => {
                for n in self.notifications.drain(..) {
                    if let Some(tx) = &self.dbus_sender {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            if let Err(err) =
                                tx.send(subscriptions::dbus::Input::Dismiss(n.id)).await
                            {
                                tracing::error!("{:?}", err);
                            }
                        });
                    }
                }
                Command::none()
            }
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
                self.config.do_not_disturb,
                Message::DoNotDisturb
            )
            .width(Length::Fill)]
            .padding([0, 24]);

            let settings =
                row_button(vec!["Notification Settings...".into()]).on_press(Message::Settings);

            let notifications = if self.notifications.len() == 0 {
                row![container(
                    column![text_icon(&self.icon_name, 40), "No Notifications"]
                        .align_items(Alignment::Center)
                )
                .width(Length::Fill)
                .align_x(Horizontal::Center)]
                .spacing(12)
            } else {
                let mut notifs: Vec<Element<_>> = Vec::with_capacity(self.notifications.len());
                notifs.push(
                    column![cosmic::widget::button(Button::Text)
                        .custom(vec![text(fl!("clear-all")).size(14).into()])
                        .on_press(Message::ClearAll)]
                    .align_items(Alignment::End)
                    .width(Length::Fill)
                    .into(),
                );
                for n in self.notifications.iter().rev() {
                    let app_name = text(if n.app_name.len() > 24 {
                        Cow::from(format!(
                            "{:.26}...",
                            n.app_name.lines().next().unwrap_or_default()
                        ))
                    } else {
                        Cow::from(&n.app_name)
                    })
                    .size(12)
                    .width(Length::Fill);
                    let urgency = n.urgency();

                    let duration_since = text(duration_ago_msg(n)).size(12);

                    notifs.push(
                        cosmic::widget::button(Button::Custom {
                            active: Box::new(move |t| {
                                let style = if urgency > 1 {
                                    Button::Primary
                                } else {
                                    Button::Secondary
                                };
                                let mut a = t.active(&style);
                                a.border_radius = 8.0.into();
                                a
                            }),
                            hover: Box::new(move |t| {
                                let style = if urgency > 1 {
                                    Button::Primary
                                } else {
                                    Button::Secondary
                                };
                                let mut a = t.hovered(&style);
                                a.border_radius = 8.0.into();
                                a
                            }),
                        })
                        .custom(vec![column!(
                            match n.image() {
                                Some(cosmic_notifications_util::Image::File(path)) => {
                                    row![icon(path.as_path(), 16), app_name, duration_since]
                                        .spacing(8)
                                        .align_items(Alignment::Center)
                                }
                                Some(cosmic_notifications_util::Image::Name(name)) => {
                                    row![icon(name.as_str(), 16), app_name, duration_since]
                                        .spacing(8)
                                        .align_items(Alignment::Center)
                                }
                                Some(cosmic_notifications_util::Image::Data {
                                    width,
                                    height,
                                    data,
                                }) => {
                                    let handle =
                                        image::Handle::from_pixels(*width, *height, data.clone());
                                    row![icon(handle, 16), app_name, duration_since]
                                        .spacing(8)
                                        .align_items(Alignment::Center)
                                }
                                None => row![app_name, duration_since],
                            },
                            text(if n.summary.len() > 77 {
                                Cow::from(format!(
                                    "{:.80}...",
                                    n.summary.lines().next().unwrap_or_default()
                                ))
                            } else {
                                Cow::from(&n.summary)
                            })
                            .size(14)
                            .width(Length::Fixed(300.0)),
                            text(if n.body.len() > 77 {
                                Cow::from(format!(
                                    "{:.80}...",
                                    n.body.lines().next().unwrap_or_default()
                                ))
                            } else {
                                Cow::from(&n.body)
                            })
                            .size(12)
                            .width(Length::Fixed(300.0)),
                        )
                        .spacing(8)
                        .into()])
                        .padding(16)
                        .on_press(Message::Dismissed(n.id))
                        .into(),
                    );
                }
                row!(scrollable(
                    Column::with_children(notifs)
                        .spacing(8)
                        .width(Length::Shrink)
                        .height(Length::Shrink),
                )
                .width(Length::Shrink)
                .height(Length::Shrink))
                .width(Length::Shrink)
            };

            let main_content = column![
                divider::horizontal::light(),
                notifications,
                divider::horizontal::light()
            ]
            .padding([0, 24])
            .spacing(12);

            let content = column![do_not_disturb, main_content, settings]
                .align_items(Alignment::Start)
                .spacing(12)
                .padding([12, 0]);

            self.applet_helper.popup_container(content).into()
        }
    }
}

// todo put into libcosmic doing so will fix the row_button's boarder radius
fn row_button(
    mut content: Vec<Element<Message>>,
) -> cosmic::iced::widget::Button<Message, Renderer> {
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

fn duration_ago_msg(notification: &Notification) -> String {
    if let Some(d) = notification.duration_since() {
        let min = d.as_secs() / 60;
        let hrs = min / 60;
        if hrs > 0 {
            fl!("hours-ago", HashMap::from_iter(vec![("duration", hrs)]))
        } else {
            fl!("minutes-ago", HashMap::from_iter(vec![("duration", min)]))
        }
    } else {
        format!("")
    }
}
