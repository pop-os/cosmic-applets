use crate::backlight::{
    screen_backlight_subscription, ScreenBacklightRequest, ScreenBacklightUpdate,
};
use crate::config;
use crate::fl;
use crate::power_daemon::{
    power_profile_subscription, Power, PowerProfileRequest, PowerProfileUpdate,
};
use crate::upower_device::{device_subscription, DeviceDbusEvent};
use crate::upower_kbdbacklight::{
    kbd_backlight_subscription, KeyboardBacklightRequest, KeyboardBacklightUpdate,
};
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::Color;
use cosmic::iced::{
    widget::{column, container, row, slider, text},
    window, Alignment, Application, Command, Length, Subscription,
};
use cosmic::iced_runtime::core::layout::Limits;
use cosmic::iced_style::application::{self, Appearance};
use cosmic::theme::Svg;
use cosmic::widget::{button, divider, icon};
use cosmic::{Element, Theme};
use cosmic_applet::{applet_button_theme, CosmicAppletHelper};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};

use log::error;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

// XXX improve
// TODO: time to empty varies? needs averaging?
fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs > 60 {
        let min = secs / 60;
        if min > 60 {
            format!("{}:{:02}", min / 60, min % 60)
        } else {
            format!("{}{}", min, fl!("minutes"))
        }
    } else {
        format!("{}{}", secs, fl!("seconds"))
    }
}

pub fn run() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    CosmicBatteryApplet::run(helper.window_settings())
}

static MAX_CHARGE: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

#[derive(Clone, Default)]
struct CosmicBatteryApplet {
    icon_name: String,
    theme: Theme,
    charging_limit: bool,
    battery_percent: f64,
    time_remaining: Duration,
    kbd_brightness: f64,
    screen_brightness: f64,
    popup: Option<window::Id>,
    id_ctr: u128,
    screen_sender: Option<UnboundedSender<ScreenBacklightRequest>>,
    kbd_sender: Option<UnboundedSender<KeyboardBacklightRequest>>,
    applet_helper: CosmicAppletHelper,
    power_profile: Power,
    power_profile_sender: Option<UnboundedSender<PowerProfileRequest>>,
    timeline: Timeline,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    Update {
        icon_name: String,
        percent: f64,
        time_to_empty: i64,
    },
    SetKbdBrightness(i32),
    SetScreenBrightness(i32),
    SetChargingLimit(chain::Toggler, bool),
    UpdateKbdBrightness(f64),
    UpdateScreenBrightness(f64),
    OpenBatterySettings,
    InitKbdBacklight(UnboundedSender<KeyboardBacklightRequest>, f64),
    InitScreenBacklight(UnboundedSender<ScreenBacklightRequest>, f64),
    Errored(String),
    Ignore,
    InitProfile(UnboundedSender<PowerProfileRequest>, Power),
    Profile(Power),
    SelectProfile(Power),
    Frame(Instant),
    Theme(Theme),
}

impl Application for CosmicBatteryApplet {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let applet_helper = CosmicAppletHelper::default();
        let theme = applet_helper.theme();
        (
            CosmicBatteryApplet {
                icon_name: "battery-symbolic".to_string(),
                applet_helper,
                theme,
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        config::APP_ID.to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Frame(now) => self.timeline.now(now),
            Message::SetKbdBrightness(brightness) => {
                self.kbd_brightness = (brightness as f64 / 100.0).clamp(0., 1.);
                if let Some(tx) = &self.kbd_sender {
                    let _ = tx.send(KeyboardBacklightRequest::Set(self.kbd_brightness));
                }
            }
            Message::SetScreenBrightness(brightness) => {
                self.screen_brightness = (brightness as f64 / 100.0).clamp(0.01, 1.0);
                if let Some(tx) = &self.screen_sender {
                    let _ = tx.send(ScreenBacklightRequest::Set(self.screen_brightness));
                }
            }
            Message::SetChargingLimit(chain, enable) => {
                self.timeline.set_chain(chain).start();
                self.charging_limit = enable;
            }
            Message::OpenBatterySettings => {
                // TODO Ashley
            }
            Message::Errored(e) => {
                error!("{}", e);
            }
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    if let Some(tx) = &self.kbd_sender {
                        let _ = tx.send(KeyboardBacklightRequest::Get);
                    }
                    if let Some(tx) = &self.screen_sender {
                        let _ = tx.send(ScreenBacklightRequest::Get);
                    }

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
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);
                    if let Some(tx) = self.power_profile_sender.as_ref() {
                        let _ = tx.send(PowerProfileRequest::Get);
                    }
                    return get_popup(popup_settings);
                }
            }
            Message::Update {
                icon_name,
                percent,
                time_to_empty,
            } => {
                self.icon_name = icon_name;
                self.battery_percent = percent;
                self.time_remaining = Duration::from_secs(time_to_empty as u64);
            }
            Message::UpdateKbdBrightness(b) => {
                self.kbd_brightness = b;
            }
            Message::Ignore => {}
            Message::InitKbdBacklight(tx, brightness) => {
                let _ = tx.send(KeyboardBacklightRequest::Get);
                self.kbd_sender = Some(tx);
                self.kbd_brightness = brightness;
            }
            Message::InitScreenBacklight(tx, brightness) => {
                let _ = tx.send(ScreenBacklightRequest::Get);
                self.screen_sender = Some(tx);
                self.screen_brightness = brightness;
            }
            Message::UpdateScreenBrightness(b) => {
                self.screen_brightness = b;
            }
            Message::InitProfile(tx, profile) => {
                self.power_profile_sender.replace(tx);
                self.power_profile = profile;
            }
            Message::Profile(profile) => {
                self.power_profile = profile;
                if let Some(tx) = &self.kbd_sender {
                    let _ = tx.send(KeyboardBacklightRequest::Get);
                }
                if let Some(tx) = &self.screen_sender {
                    let _ = tx.send(ScreenBacklightRequest::Get);
                }
            }
            Message::SelectProfile(profile) => {
                if let Some(tx) = self.power_profile_sender.as_ref() {
                    let _ = tx.send(PowerProfileRequest::Set(profile));
                }
            }
            Message::Theme(t) => {
                self.theme = t;
            }
        }
        Command::none()
    }
    fn view(&self, id: window::Id) -> Element<Message> {
        if id == window::Id(0) {
            self.applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into()
        } else {
            let name = text(fl!("battery")).size(14);
            let description = text(
                if "battery-full-charging-symbolic" == self.icon_name
                    || "battery-full-charged-symbolic" == self.icon_name
                {
                    format!("{}%", self.battery_percent)
                } else {
                    format!(
                        "{} {} ({:.0}%)",
                        format_duration(self.time_remaining),
                        fl!("until-empty"),
                        self.battery_percent
                    )
                },
            )
            .size(10);
            self.applet_helper
                .popup_container(
                    column![
                        row![
                            icon(&*self.icon_name, 24).style(Svg::Symbolic),
                            column![name, description]
                        ]
                        .padding([0, 24])
                        .spacing(8)
                        .align_items(Alignment::Center),
                        container(divider::horizontal::light())
                            .width(Length::Fill)
                            .padding([0, 12]),
                        button(applet_button_theme())
                            .custom(vec![row![
                                column![
                                    text(fl!("battery")).size(14),
                                    text(fl!("battery-desc")).size(10)
                                ]
                                .width(Length::Fill),
                                icon("emblem-ok-symbolic", 12).size(12).style(
                                    match self.power_profile {
                                        Power::Battery => Svg::SymbolicActive,
                                        _ => Svg::Default,
                                    }
                                ),
                            ]
                            .align_items(Alignment::Center)
                            .into()])
                            .padding([8, 24])
                            .on_press(Message::SelectProfile(Power::Battery))
                            .width(Length::Fill),
                        button(applet_button_theme())
                            .custom(vec![row![
                                column![
                                    text(fl!("balanced")).size(14),
                                    text(fl!("balanced-desc")).size(10)
                                ]
                                .width(Length::Fill),
                                icon("emblem-ok-symbolic", 12).size(12).style(
                                    match self.power_profile {
                                        Power::Balanced => Svg::SymbolicActive,
                                        _ => Svg::Default,
                                    }
                                ),
                            ]
                            .align_items(Alignment::Center)
                            .into()])
                            .padding([8, 24])
                            .on_press(Message::SelectProfile(Power::Balanced))
                            .width(Length::Fill),
                        button(applet_button_theme())
                            .custom(vec![row![
                                column![
                                    text(fl!("performance")).size(14),
                                    text(fl!("performance-desc")).size(10)
                                ]
                                .width(Length::Fill),
                                icon("emblem-ok-symbolic", 12).size(12).style(
                                    match self.power_profile {
                                        Power::Performance => Svg::SymbolicActive,
                                        _ => Svg::Default,
                                    }
                                ),
                            ]
                            .align_items(Alignment::Center)
                            .into()])
                            .padding([8, 24])
                            .on_press(Message::SelectProfile(Power::Performance))
                            .width(Length::Fill),
                        container(divider::horizontal::light())
                            .width(Length::Fill)
                            .padding([0, 12]),
                        container(
                            anim!(
                                //toggler
                                MAX_CHARGE,
                                &self.timeline,
                                fl!("max-charge"),
                                self.charging_limit,
                                Message::SetChargingLimit,
                            )
                            .text_size(14)
                            .width(Length::Fill)
                        )
                        .padding([0, 24])
                        .width(Length::Fill),
                        container(divider::horizontal::light())
                            .width(Length::Fill)
                            .padding([0, 12]),
                        row![
                            icon("display-brightness-symbolic", 24).style(Svg::Symbolic),
                            slider(
                                1..=100,
                                (self.screen_brightness * 100.0) as i32,
                                Message::SetScreenBrightness
                            ),
                            text(format!("{:.0}%", self.screen_brightness * 100.0))
                                .size(16)
                                .width(Length::Fixed(40.0))
                                .horizontal_alignment(Horizontal::Right)
                        ]
                        .padding([0, 24])
                        .spacing(12),
                        row![
                            icon("keyboard-brightness-symbolic", 24).style(Svg::Symbolic),
                            slider(
                                0..=100,
                                (self.kbd_brightness * 100.0) as i32,
                                Message::SetKbdBrightness
                            ),
                            text(format!("{:.0}%", self.kbd_brightness * 100.0))
                                .size(16)
                                .width(Length::Fixed(40.0))
                                .horizontal_alignment(Horizontal::Right)
                        ]
                        .padding([0, 24])
                        .spacing(12),
                        container(divider::horizontal::light())
                            .width(Length::Fill)
                            .padding([0, 12]),
                        button(applet_button_theme())
                            .custom(vec![text(fl!("power-settings"))
                                .size(14)
                                .width(Length::Fill)
                                .into()])
                            .on_press(Message::OpenBatterySettings)
                            .width(Length::Fill)
                            .padding([8, 24])
                    ]
                    .spacing(8)
                    .padding([8, 0]),
                )
                .into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            self.applet_helper.theme_subscription(0).map(Message::Theme),
            device_subscription(0).map(
                |DeviceDbusEvent::Update {
                     icon_name,
                     percent,
                     time_to_empty,
                 }| Message::Update {
                    icon_name,
                    percent,
                    time_to_empty,
                },
            ),
            kbd_backlight_subscription(0).map(|event| match event {
                KeyboardBacklightUpdate::Update(b) => Message::UpdateKbdBrightness(b),
                KeyboardBacklightUpdate::Init(tx, b) => Message::InitKbdBacklight(tx, b),
            }),
            screen_backlight_subscription(0).map(|e| match e {
                ScreenBacklightUpdate::Update(b) => Message::UpdateScreenBrightness(b),
                ScreenBacklightUpdate::Init(tx, b) => Message::InitScreenBacklight(tx, b),
            }),
            power_profile_subscription(0).map(|event| match event {
                PowerProfileUpdate::Update { profile } => Message::Profile(profile),
                PowerProfileUpdate::Init(tx, p) => Message::InitProfile(p, tx),
                PowerProfileUpdate::Error(e) => Message::Errored(e), // TODO: handle error
            }),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
        ])
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn close_requested(&self, _id: window::Id) -> Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(Box::new(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        }))
    }
}
