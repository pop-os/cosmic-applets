mod localize;
mod subscriptions;

use cosmic::applet::{menu_button, menu_control_padding, padded_control};
use cosmic::cosmic_config::{config_subscription, Config, CosmicConfigEntry};
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::Limits;
use cosmic::iced::{
    widget::{column, row, text},
    window, Alignment, Length, Subscription,
};
use cosmic::iced_core::alignment::Horizontal;
use cosmic::Command;

use cosmic::iced_style::application;

use cosmic::iced_widget::{scrollable, Column};
use cosmic::widget::{button, container, divider, icon};
use cosmic::{Element, Theme};
use cosmic_notifications_config::NotificationsConfig;
use cosmic_notifications_util::{Image, Notification};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use tokio::sync::mpsc::Sender;
use tracing::info;

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt::init();
    // Prepare i18n
    localize::localize();

    info!("Notifications applet");

    cosmic::applet::run::<Notifications>(false, ())
}

static DO_NOT_DISTURB: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

#[derive(Default)]
struct Notifications {
    core: cosmic::app::Core,
    config: NotificationsConfig,
    config_helper: Option<Config>,
    icon_name: String,
    popup: Option<window::Id>,
    id_ctr: u128,
    // notifications: Vec<Notification>,
    timeline: Timeline,
    dbus_sender: Option<Sender<subscriptions::dbus::Input>>,
    cards: Vec<(id::Cards, Vec<Notification>, bool, String, String, String)>,
}

impl Notifications {
    fn update_cards(&mut self, id: id::Cards) {
        if let Some((id, _, card_value, ..)) = self.cards.iter_mut().find(|c| c.0 == id) {
            let chain = if *card_value {
                chain::Cards::on(id.clone(), 1.)
            } else {
                chain::Cards::off(id.clone(), 1.)
            };
            self.timeline.set_chain(chain);
            self.timeline.start();
        }
    }

    fn update_icon(&mut self) {
        self.icon_name = if self.config.do_not_disturb {
            "cosmic-applet-notification-disabled-symbolic"
        } else if self.cards.is_empty() {
            "cosmic-applet-notification-symbolic"
        } else {
            "cosmic-applet-notification-new-symbolic"
        }
        .to_string();
    }
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    DoNotDisturb(chain::Toggler, bool),
    Settings,
    Frame(Instant),
    NotificationEvent(Notification),
    Config(NotificationsConfig),
    DbusEvent(subscriptions::dbus::Output),
    Dismissed(u32),
    ClearAll(String),
    CardsToggled(String, bool),
}

impl cosmic::Application for Notifications {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletNotifications";

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (
        Self,
        cosmic::iced::Command<cosmic::app::Message<Self::Message>>,
    ) {
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
        let mut _self = Notifications {
            core,
            config_helper: helper,
            config,
            ..Default::default()
        };
        _self.update_icon();
        (_self, Command::none())
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
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

    fn update(
        &mut self,
        message: Self::Message,
    ) -> cosmic::iced::Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::Frame(now) => {
                self.timeline.now(now);
            }
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
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
                        .min_width(1.0)
                        .max_width(444.0)
                        .min_height(100.0)
                        .max_height(900.0);
                    return get_popup(popup_settings);
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
            }
            Message::Settings => {
                let _ = process::Command::new("cosmic-settings notifications").spawn();
            }
            Message::NotificationEvent(n) => {
                if let Some(c) = self
                    .cards
                    .iter_mut()
                    .find(|c| c.1.iter().any(|notif| n.app_name == notif.app_name))
                {
                    if let Some(notif) = c.1.iter_mut().find(|notif| n.id == notif.id) {
                        *notif = n;
                    } else {
                        c.1.push(n);
                        c.3 = fl!(
                            "show-more",
                            HashMap::from_iter(vec![("more", c.1.len().saturating_sub(1))])
                        );
                    }
                } else {
                    self.cards.push((
                        id::Cards::new(n.app_name.clone()),
                        vec![n],
                        false,
                        fl!("show-more", HashMap::from_iter(vec![("more", "1")])),
                        fl!("show-less"),
                        fl!("clear-all"),
                    ));
                }
            }
            Message::Config(config) => {
                self.config = config;
            }
            Message::Dismissed(id) => {
                info!("Dismissed {}", id);
                for c in &mut self.cards {
                    c.1.retain(|n| n.id != id);
                }
                self.cards.retain(|c| !c.1.is_empty());

                if let Some(tx) = &self.dbus_sender {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Err(err) = tx.send(subscriptions::dbus::Input::Dismiss(id)).await {
                            tracing::error!("{:?}", err);
                        }
                    });
                }
            }
            Message::DbusEvent(e) => match e {
                subscriptions::dbus::Output::Ready(tx) => {
                    self.dbus_sender.replace(tx);
                }
                subscriptions::dbus::Output::CloseEvent(id) => {
                    for c in &mut self.cards {
                        c.1.retain(|n| n.id != id);
                        c.3 = fl!(
                            "show-more",
                            HashMap::from_iter(vec![("more", c.1.len().saturating_sub(1))])
                        );
                    }
                    self.cards.retain(|c| !c.1.is_empty());
                }
            },
            Message::ClearAll(app_name) => {
                if let Some(pos) = self
                    .cards
                    .iter_mut()
                    .position(|c| c.1.iter().any(|notif| app_name == notif.app_name))
                {
                    for n in self.cards.remove(pos).1 {
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
                }
            }
            Message::CardsToggled(name, expanded) => {
                let id = if let Some((id, _, n_expanded, ..)) = self
                    .cards
                    .iter_mut()
                    .find(|c| c.1.iter().any(|notif| name == notif.app_name))
                {
                    *n_expanded = expanded;
                    id.clone()
                } else {
                    return Command::none();
                };
                self.update_cards(id);
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
        };
        self.update_icon();
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        self.core
            .applet
            .icon_button(&self.icon_name)
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let do_not_disturb = padded_control(row![anim!(
            DO_NOT_DISTURB,
            &self.timeline,
            fl!("do-not-disturb"),
            self.config.do_not_disturb,
            Message::DoNotDisturb
        )
        .text_size(14)
        .width(Length::Fill)]);

        let settings =
            menu_button(text(fl!("notification-settings")).size(14)).on_press(Message::Settings);

        let notifications = if self.cards.is_empty() {
            row![container(
                column![
                    text_icon("cosmic-applet-notification-symbolic", 40),
                    text(&fl!("no-notifications")).size(14)
                ]
                .align_items(Alignment::Center)
            )
            .width(Length::Fill)
            .align_x(Horizontal::Center)]
            .spacing(12)
        } else {
            let mut notifs: Vec<Element<_>> = Vec::with_capacity(self.cards.len());

            for c in self.cards.iter().rev() {
                if c.1.is_empty() {
                    continue;
                }
                let name = c.1[0].app_name.clone();
                let notif_elems: Vec<_> = c
                    .1
                    .iter()
                    .rev()
                    .map(|n| {
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

                        let duration_since = text(duration_ago_msg(n)).size(10);

                        let close_notif = button(
                            icon::from_name("window-close-symbolic")
                                .size(16)
                                .symbolic(true),
                        )
                        .on_press(Message::Dismissed(n.id))
                        .style(cosmic::theme::Button::Text);
                        Element::from(
                            column!(
                                match n.image() {
                                    Some(cosmic_notifications_util::Image::File(path)) => {
                                        row![
                                            icon::from_path(PathBuf::from(path)).icon().size(16),
                                            app_name,
                                            duration_since,
                                            close_notif
                                        ]
                                        .spacing(8)
                                        .align_items(Alignment::Center)
                                    }
                                    Some(cosmic_notifications_util::Image::Name(name)) => {
                                        row![
                                            icon::from_name(name.as_str()).size(16),
                                            app_name,
                                            duration_since,
                                            close_notif
                                        ]
                                        .spacing(8)
                                        .align_items(Alignment::Center)
                                    }
                                    Some(cosmic_notifications_util::Image::Data {
                                        width,
                                        height,
                                        data,
                                    }) => {
                                        row![
                                            icon::from_raster_pixels(*width, *height, data.clone())
                                                .icon()
                                                .size(16),
                                            app_name,
                                            duration_since,
                                            close_notif
                                        ]
                                        .spacing(8)
                                        .align_items(Alignment::Center)
                                    }
                                    None => row![app_name, duration_since, close_notif]
                                        .spacing(8)
                                        .align_items(Alignment::Center),
                                },
                                column![
                                    text(n.summary.lines().next().unwrap_or_default())
                                        .width(Length::Fill)
                                        .size(14),
                                    text(n.body.lines().next().unwrap_or_default())
                                        .width(Length::Fill)
                                        .size(12)
                                ]
                            )
                            .width(Length::Fill),
                        )
                    })
                    .collect();
                let show_more_icon = c.1.last().and_then(|n| {
                    info!("app_icon: {:?}", &n.app_icon);
                    if n.app_icon.is_empty() {
                        match n.image().cloned() {
                            Some(Image::File(p)) => Some(cosmic::widget::icon::from_path(p)),
                            Some(Image::Name(name)) => {
                                Some(cosmic::widget::icon::from_name(name).handle())
                            }
                            Some(Image::Data {
                                width,
                                height,
                                data,
                            }) => Some(cosmic::widget::icon::from_raster_pixels(
                                width, height, data,
                            )),
                            None => None,
                        }
                    } else if let Some(path) = url::Url::parse(&n.app_icon)
                        .ok()
                        .and_then(|u| u.to_file_path().ok())
                    {
                        Some(cosmic::widget::icon::from_path(path))
                    } else {
                        Some(cosmic::widget::icon::from_name(n.app_icon.clone()).handle())
                    }
                });
                let card_list = anim!(
                    //cards
                    c.0.clone(),
                    &self.timeline,
                    notif_elems,
                    Message::ClearAll(name.clone()),
                    move |_, e| Message::CardsToggled(name.clone(), e),
                    &c.3,
                    &c.4,
                    &c.5,
                    show_more_icon,
                    c.2,
                );
                notifs.push(card_list.into());
            }

            row!(scrollable(
                Column::with_children(notifs)
                    .spacing(8)
                    .height(Length::Shrink),
            )
            .height(Length::Shrink))
            .padding(menu_control_padding())
        };

        let main_content = column![
            padded_control(divider::horizontal::default()),
            notifications,
            padded_control(divider::horizontal::default())
        ];

        let content = column![do_not_disturb, main_content, settings]
            .align_items(Alignment::Start)
            .padding([8, 0]);

        self.core.applet.popup_container(content).into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}

fn text_icon(name: &str, size: u16) -> cosmic::widget::Icon {
    icon::from_name(name).size(size).symbolic(true).icon()
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
        String::new()
    }
}
