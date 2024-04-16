use crate::backend::{power_profile_subscription, Power, PowerProfileRequest, PowerProfileUpdate};
use crate::backlight::{
    screen_backlight_subscription, ScreenBacklightRequest, ScreenBacklightUpdate,
};
use crate::config;
use crate::dgpu::{dgpu_subscription, Entry, GpuUpdate};
use crate::fl;
use crate::upower_device::{device_subscription, DeviceDbusEvent};
use crate::upower_kbdbacklight::{
    kbd_backlight_subscription, KeyboardBacklightRequest, KeyboardBacklightUpdate,
};
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::applet::token::subscription::{
    activation_token_subscription, TokenRequest, TokenUpdate,
};
use cosmic::applet::{menu_button, padded_control};
use cosmic::cctk::sctk::reexports::calloop;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::{
    widget::{column, container, row, slider, text},
    window, Alignment, Length, Subscription,
};
use cosmic::iced_core::alignment::Vertical;
use cosmic::iced_core::{Background, Border, Color, Shadow};
use cosmic::iced_runtime::core::layout::Limits;
use cosmic::iced_style::application;
use cosmic::iced_widget::{Column, Row};
use cosmic::widget::{divider, horizontal_space, icon, scrollable, vertical_space};
use cosmic::Command;
use cosmic::{Element, Theme};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};

use std::collections::HashMap;
use std::path::PathBuf;
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
    cosmic::applet::run::<CosmicBatteryApplet>(true, ())
}

static MAX_CHARGE: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

#[derive(Clone, Default)]
struct GPUData {
    name: String,
    toggled: bool,
    app_list: Option<Vec<Entry>>,
}

#[derive(Clone, Default)]
struct CosmicBatteryApplet {
    core: cosmic::app::Core,
    icon_name: String,
    display_icon_name: String,
    charging_limit: bool,
    battery_percent: f64,
    on_battery: bool,
    gpus: HashMap<PathBuf, GPUData>,
    time_remaining: Duration,
    kbd_brightness: Option<f64>,
    screen_brightness: f64,
    popup: Option<window::Id>,
    screen_sender: Option<UnboundedSender<ScreenBacklightRequest>>,
    kbd_sender: Option<UnboundedSender<KeyboardBacklightRequest>>,
    power_profile: Power,
    power_profile_sender: Option<UnboundedSender<PowerProfileRequest>>,
    timeline: Timeline,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
}

impl CosmicBatteryApplet {
    fn update_battery(&mut self, mut percent: f64, on_battery: bool) {
        percent = percent.clamp(0.0, 100.0);
        self.on_battery = on_battery;
        self.battery_percent = percent;
        let battery_percent = if self.battery_percent > 95.0 && !self.charging_limit {
            100
        } else if self.battery_percent > 80.0 && !self.charging_limit {
            90
        } else if self.battery_percent > 65.0 {
            80
        } else if self.battery_percent > 35.0 {
            50
        } else if self.battery_percent > 20.0 {
            35
        } else if self.battery_percent > 14.0 {
            20
        } else if self.battery_percent > 9.0 {
            10
        } else if self.battery_percent > 5.0 {
            5
        } else {
            0
        };
        let limited = if self.charging_limit { "limited-" } else { "" };
        let charging = if on_battery { "" } else { "charging-" };
        self.icon_name =
            format!("cosmic-applet-battery-level-{battery_percent}-{limited}{charging}symbolic",);
    }

    fn update_display(&mut self, mut percent: f64) {
        percent = percent.clamp(0.01, 1.0);
        self.screen_brightness = percent;
        let screen_brightness = if self.screen_brightness < 0.011 {
            "off"
        } else if self.screen_brightness < 0.333 {
            "low"
        } else if self.screen_brightness < 0.666 {
            "medium"
        } else {
            "high"
        }
        .to_string();

        self.display_icon_name =
            format!("cosmic-applet-battery-display-brightness-{screen_brightness}-symbolic",);
    }

    fn set_charging_limit(&mut self, limit: bool) {
        self.charging_limit = limit;
        self.update_battery(self.battery_percent, self.on_battery);
    }
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    Update {
        on_battery: bool,
        percent: f64,
        time_to_empty: i64,
    },
    SetKbdBrightness(i32),
    SetScreenBrightness(i32),
    SetChargingLimit(chain::Toggler, bool),
    UpdateKbdBrightness(Option<f64>),
    UpdateScreenBrightness(f64),
    InitKbdBacklight(UnboundedSender<KeyboardBacklightRequest>),
    InitScreenBacklight(UnboundedSender<ScreenBacklightRequest>, f64),
    GpuOn(PathBuf, String, Option<Vec<Entry>>),
    GpuOff(PathBuf),
    ToggleGpuApps(PathBuf),
    Errored(String),
    InitProfile(UnboundedSender<PowerProfileRequest>, Power),
    Profile(Power),
    SelectProfile(Power),
    Frame(Instant),
    Token(TokenUpdate),
    OpenSettings,
}

impl cosmic::Application for CosmicBatteryApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (
        Self,
        cosmic::iced::Command<cosmic::app::Message<Self::Message>>,
    ) {
        (
            Self {
                core,
                icon_name: "battery-symbolic".to_string(),
                display_icon_name: "display-brightness-symbolic".to_string(),
                token_tx: None,

                ..Default::default()
            },
            Command::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn update(
        &mut self,
        message: Self::Message,
    ) -> cosmic::iced::Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::Frame(now) => self.timeline.now(now),
            Message::SetKbdBrightness(brightness) => {
                let brightness = (brightness as f64 / 100.0).clamp(0., 1.);
                self.kbd_brightness = Some(brightness);
                if let Some(tx) = &self.kbd_sender {
                    let _ = tx.send(KeyboardBacklightRequest::Set(brightness));
                }
            }
            Message::SetScreenBrightness(brightness) => {
                self.update_display((brightness as f64 / 100.0).clamp(0.01, 1.0));
                if let Some(tx) = &self.screen_sender {
                    let _ = tx.send(ScreenBacklightRequest::Set(self.screen_brightness));
                }
            }
            Message::SetChargingLimit(chain, enable) => {
                self.timeline.set_chain(chain).start();
                self.set_charging_limit(enable);
            }
            Message::Errored(why) => {
                tracing::error!("{}", why);
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

                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        window::Id::MAIN,
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
                on_battery,
                percent,
                time_to_empty,
            } => {
                self.update_battery(percent, on_battery);
                self.time_remaining = Duration::from_secs(time_to_empty as u64);
            }
            Message::UpdateKbdBrightness(b) => {
                self.kbd_brightness = b;
            }
            Message::InitKbdBacklight(tx) => {
                self.kbd_sender = Some(tx);
            }
            Message::InitScreenBacklight(tx, brightness) => {
                let _ = tx.send(ScreenBacklightRequest::Get);
                self.screen_sender = Some(tx);
                self.update_display(brightness);
            }
            Message::UpdateScreenBrightness(b) => {
                self.update_display(b);
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
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
            Message::OpenSettings => {
                let exec = "cosmic-settings power".to_string();
                if let Some(tx) = self.token_tx.as_ref() {
                    let _ = tx.send(TokenRequest {
                        app_id: Self::APP_ID.to_string(),
                        exec,
                    });
                } else {
                    tracing::error!("Wayland tx is None");
                };
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
                    cmd.arg("power");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    cosmic::process::spawn(cmd);
                }
            },
            Message::GpuOn(path, name, app_list) => {
                let toggled = self
                    .gpus
                    .get(&path)
                    .map(|data| data.toggled)
                    .unwrap_or_default();
                self.gpus.insert(
                    path,
                    GPUData {
                        name,
                        app_list,
                        toggled,
                    },
                );
            }
            Message::GpuOff(path) => {
                self.gpus.remove(&path);
            }
            Message::ToggleGpuApps(path) => {
                if let Some(data) = self.gpus.get_mut(&path) {
                    data.toggled = !data.toggled;
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        let btn = self
            .core
            .applet
            .icon_button(&self.icon_name)
            .on_press(Message::TogglePopup)
            .into();

        if !self.gpus.is_empty() {
            let dot = container(vertical_space(Length::Fixed(0.0)))
                .padding(2.0)
                .style(<Theme as container::StyleSheet>::Style::Custom(Box::new(
                    |theme| container::Appearance {
                        text_color: Some(Color::TRANSPARENT),
                        background: Some(Background::Color(theme.cosmic().accent_color().into())),
                        border: Border {
                            radius: 2.0.into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        shadow: Shadow::default(),
                        icon_color: Some(Color::TRANSPARENT),
                    },
                )))
                .into();

            match self.core.applet.anchor {
                PanelAnchor::Left | PanelAnchor::Right => Column::with_children(vec![btn, dot])
                    .align_items(Alignment::Center)
                    .into(),
                PanelAnchor::Top | PanelAnchor::Bottom => Row::with_children(vec![btn, dot])
                    .align_items(Alignment::Center)
                    .into(),
            }
        } else {
            btn
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let name = text(fl!("battery")).size(14);
        let description = text(if !self.on_battery {
            format!("{}%", self.battery_percent)
        } else {
            format!(
                "{} {} ({:.0}%)",
                format_duration(self.time_remaining),
                fl!("until-empty"),
                self.battery_percent
            )
        })
        .size(10);

        let mut content = vec![
            padded_control(
                row![
                    icon::from_name(&*self.icon_name).size(24).symbolic(true),
                    column![name, description]
                ]
                .spacing(8)
                .align_items(Alignment::Center),
            )
            .into(),
            padded_control(divider::horizontal::default()).into(),
            menu_button(
                row![
                    column![
                        text(fl!("battery")).size(14),
                        text(fl!("battery-desc")).size(10)
                    ]
                    .width(Length::Fill),
                    if matches!(self.power_profile, Power::Battery) {
                        container(
                            icon::from_name("emblem-ok-symbolic")
                                .size(12)
                                .symbolic(true),
                        )
                    } else {
                        container(horizontal_space(1.0))
                    }
                ]
                .align_items(Alignment::Center),
            )
            .on_press(Message::SelectProfile(Power::Battery))
            .into(),
            menu_button(
                row![
                    column![
                        text(fl!("balanced")).size(14),
                        text(fl!("balanced-desc")).size(10)
                    ]
                    .width(Length::Fill),
                    if matches!(self.power_profile, Power::Balanced) {
                        container(
                            icon::from_name("emblem-ok-symbolic")
                                .size(12)
                                .symbolic(true),
                        )
                    } else {
                        container(horizontal_space(1.0))
                    }
                ]
                .align_items(Alignment::Center),
            )
            .on_press(Message::SelectProfile(Power::Balanced))
            .into(),
            menu_button(
                row![
                    column![
                        text(fl!("performance")).size(14),
                        text(fl!("performance-desc")).size(10)
                    ]
                    .width(Length::Fill),
                    if matches!(self.power_profile, Power::Performance) {
                        container(
                            icon::from_name("emblem-ok-symbolic")
                                .size(12)
                                .symbolic(true),
                        )
                    } else {
                        container(horizontal_space(1.0))
                    }
                ]
                .align_items(Alignment::Center),
            )
            .on_press(Message::SelectProfile(Power::Performance))
            .into(),
            padded_control(divider::horizontal::default()).into(),
            padded_control(
                anim!(
                    //toggler
                    MAX_CHARGE,
                    &self.timeline,
                    fl!("max-charge"),
                    self.charging_limit,
                    Message::SetChargingLimit,
                )
                .text_size(14)
                .width(Length::Fill),
            )
            .into(),
            padded_control(divider::horizontal::default()).into(),
            padded_control(
                row![
                    icon::from_name(self.display_icon_name.as_str())
                        .size(24)
                        .symbolic(true),
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
                .spacing(12),
            )
            .into(),
        ];

        if let Some(kbd_brightness) = self.kbd_brightness {
            content.push(
                padded_control(
                    row![
                        icon::from_name("keyboard-brightness-symbolic")
                            .size(24)
                            .symbolic(true),
                        slider(
                            0..=100,
                            (kbd_brightness * 100.0) as i32,
                            Message::SetKbdBrightness
                        ),
                        text(format!("{:.0}%", kbd_brightness * 100.0))
                            .size(16)
                            .width(Length::Fixed(40.0))
                            .horizontal_alignment(Horizontal::Right)
                    ]
                    .spacing(12),
                )
                .into(),
            );
        }

        content.push(padded_control(divider::horizontal::default()).into());

        if !self.gpus.is_empty() {
            content.push(
                padded_control(
                    row![
                        text(fl!("dgpu-running"))
                            .size(16)
                            .width(Length::Fill)
                            .horizontal_alignment(Horizontal::Left),
                        container(vertical_space(Length::Fixed(0.0)))
                            .padding(4)
                            .style(<Theme as container::StyleSheet>::Style::Custom(Box::new(
                                |theme| container::Appearance {
                                    text_color: Some(Color::TRANSPARENT),
                                    background: Some(Background::Color(
                                        theme.cosmic().accent_color().into(),
                                    )),
                                    border: Border {
                                        radius: 4.0.into(),
                                        width: 0.0,
                                        color: Color::TRANSPARENT,
                                    },
                                    shadow: Default::default(),
                                    icon_color: Some(Color::TRANSPARENT),
                                },
                            ))),
                    ]
                    .align_items(Alignment::Center),
                )
                .into(),
            );
            content.push(padded_control(divider::horizontal::default()).into());
        }

        for (key, gpu) in &self.gpus {
            if gpu.app_list.is_none() {
                continue;
            }

            content.push(
                menu_button(row![
                    text(fl!(
                        "dgpu-applications",
                        gpu_name = if self.gpus.len() == 1 {
                            String::new()
                        } else {
                            format!("\"{}\"", gpu.name)
                        }
                    ))
                    .size(14)
                    .width(Length::Fill)
                    .height(Length::Fixed(24.0))
                    .vertical_alignment(Vertical::Center),
                    container(
                        icon::from_name(if gpu.toggled {
                            "go-down-symbolic"
                        } else {
                            "go-up-symbolic"
                        })
                        .size(14)
                        .symbolic(true)
                    )
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .width(Length::Fixed(24.0))
                    .height(Length::Fixed(24.0)),
                ])
                .on_press(Message::ToggleGpuApps(key.clone()))
                .into(),
            );

            if gpu.toggled {
                let app_list = gpu.app_list.as_ref().unwrap();
                let mut list_apps = Vec::with_capacity(app_list.len());
                for app in app_list {
                    list_apps.push(
                        padded_control(
                            row![
                                if let Some(icon) = &app.icon {
                                    container(icon::from_name(&**icon).size(12).symbolic(true))
                                } else {
                                    container(horizontal_space(12.0))
                                },
                                column![text(&app.name).size(14), text(&app.secondary).size(10)]
                                    .width(Length::Fill),
                            ]
                            .spacing(8)
                            .align_items(Alignment::Center),
                        )
                        .into(),
                    );
                }
                content.push(
                    scrollable(Column::with_children(list_apps))
                        .height(Length::Fixed(300.0))
                        .into(),
                );
            }
            content.push(padded_control(divider::horizontal::default()).into());
        }

        content.push(
            menu_button(text(fl!("power-settings")).size(14).width(Length::Fill))
                .on_press(Message::OpenSettings)
                .into(),
        );

        self.core
            .applet
            .popup_container(Column::with_children(content).padding([8, 0]))
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            device_subscription(0).map(
                |DeviceDbusEvent::Update {
                     on_battery,
                     percent,
                     time_to_empty,
                 }| Message::Update {
                    on_battery,
                    percent,
                    time_to_empty,
                },
            ),
            kbd_backlight_subscription(0).map(|event| match event {
                KeyboardBacklightUpdate::Brightness(b) => Message::UpdateKbdBrightness(b),
                KeyboardBacklightUpdate::Sender(tx) => Message::InitKbdBacklight(tx),
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
            dgpu_subscription(0).map(|event| match event {
                GpuUpdate::On(path, name, list) => Message::GpuOn(path, name, list),
                GpuUpdate::Off(path) => Message::GpuOff(path),
            }),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
            activation_token_subscription(0).map(Message::Token),
        ])
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
