// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    backend::{
        Power, PowerProfileRequest, PowerProfileUpdate, get_charging_limit,
        power_profile_subscription, set_charging_limit, unset_charging_limit,
    },
    dgpu::{Entry, GpuUpdate, dgpu_subscription},
    fl,
};
use cosmic::{
    Element, Task, app,
    applet::{
        cosmic_panel_config::PanelAnchor,
        menu_button, padded_control,
        token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    },
    cctk::sctk::reexports::calloop,
    cosmic_theme::Spacing,
    iced::{
        Length, Subscription,
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        widget::{Column, column, container, row},
        window,
    },
    iced_core::{Alignment, Background, Border, Color, Shadow},
    surface,
    theme::{self, Button},
    widget::{button, divider, horizontal_space, icon, scrollable, slider, text, vertical_space},
};
use cosmic_applets_config::battery::BatteryAppletConfig;
use cosmic_config::{Config, CosmicConfigEntry};
use cosmic_settings_daemon_subscription as settings_daemon;
use cosmic_settings_upower_subscription::{
    device::{DeviceDbusEvent, device_subscription},
    kbdbacklight::{KeyboardBacklightRequest, KeyboardBacklightUpdate, kbd_backlight_subscription},
};

use cosmic_time::{Instant, Timeline, anim, chain, id};

use rustc_hash::FxHashMap;
use std::{path::PathBuf, sync::LazyLock, time::Duration};
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
    cosmic::applet::run::<CosmicBatteryApplet>(())
}

static MAX_CHARGE: LazyLock<id::Toggler> = LazyLock::new(id::Toggler::unique);

#[derive(Clone, Default)]
struct GPUData {
    name: String,
    toggled: bool,
    app_list: Option<Vec<Entry>>,
}

#[derive(Clone, Default)]
struct CosmicBatteryApplet {
    core: cosmic::app::Core,
    config: BatteryAppletConfig,
    icon_name: String,
    display_icon_name: String,
    charging_limit: Option<bool>,
    battery_percent: f64,
    on_battery: bool,
    gpus: FxHashMap<PathBuf, GPUData>,
    update_trigger: Option<UnboundedSender<()>>,
    time_remaining: Duration,
    max_kbd_brightness: Option<i32>,
    kbd_brightness: Option<i32>,
    max_screen_brightness: Option<i32>,
    screen_brightness: Option<i32>,
    popup: Option<window::Id>,
    settings_daemon_sender: Option<UnboundedSender<settings_daemon::Request>>,
    kbd_sender: Option<UnboundedSender<KeyboardBacklightRequest>>,
    power_profile: Power,
    power_profile_sender: Option<UnboundedSender<PowerProfileRequest>>,
    timeline: Timeline,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    zbus_connection: Option<zbus::Connection>,
    dragging_screen_brightness: bool,
    dragging_kbd_brightness: bool,
}

impl CosmicBatteryApplet {
    fn update_battery(&mut self, mut percent: f64, on_battery: bool) {
        percent = percent.clamp(0.0, 100.0);
        self.on_battery = on_battery;
        self.battery_percent = percent;
        let battery_percent =
            if self.battery_percent > 95.0 && !self.charging_limit.unwrap_or_default() {
                100
            } else if self.battery_percent > 80.0 && !self.charging_limit.unwrap_or_default() {
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
        let limited = if self.charging_limit.unwrap_or_default() {
            "limited-"
        } else {
            ""
        };
        let charging = if on_battery { "" } else { "charging-" };
        self.icon_name =
            format!("cosmic-applet-battery-level-{battery_percent}-{limited}{charging}symbolic",);
    }

    fn screen_brightness_percent(&self) -> Option<f64> {
        let raw = self.screen_brightness? as i64;
        let max = self.max_screen_brightness?.max(1) as i64;
        if max <= 20 {
            // Coarse panels (<=20 brightness levels)
            let rung = (raw.saturating_add(1)).min(20);
            Some((5 * rung) as f64 / 100.0)
        } else {
            let p = ((raw * 100 + max / 2) / max).clamp(1, 100) as f64;
            Some(p / 100.0)
        }
    }

    fn update_display(&mut self) {
        let screen_brightness = if let Some(percent) = self.screen_brightness_percent() {
            if percent < 0.011 {
                "off"
            } else if percent < 0.333 {
                "low"
            } else if percent < 0.666 {
                "medium"
            } else {
                "high"
            }
        } else {
            "off"
        };

        self.display_icon_name =
            format!("cosmic-applet-battery-display-brightness-{screen_brightness}-symbolic",);
    }

    fn set_charging_limit(&mut self, limit: bool) {
        self.charging_limit = Some(limit);
        self.update_battery(self.battery_percent, self.on_battery);
    }
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    SetKbdBrightness(i32),
    ReleaseKbdBrightness,
    SetScreenBrightness(i32),
    SetKbdBrightnessDebounced,
    SetScreenBrightnessDebounced,
    ReleaseScreenBrightness,
    InitChargingLimit(Option<bool>),
    SetChargingLimit(chain::Toggler, bool),
    KeyboardBacklight(KeyboardBacklightUpdate),
    UpowerDevice(DeviceDbusEvent),
    GpuInit(UnboundedSender<()>),
    GpuOn(PathBuf, String, Option<Vec<Entry>>),
    GpuOff(PathBuf),
    ToggleGpuApps(PathBuf),
    Errored(String),
    InitProfile(UnboundedSender<PowerProfileRequest>, Power),
    Profile(Power),
    SelectProfile(Power),
    Frame(Instant),
    ConfigChanged(BatteryAppletConfig),
    Token(TokenUpdate),
    OpenSettings,
    SettingsDaemon(settings_daemon::Event),
    ZbusConnection(zbus::Result<zbus::Connection>),
    Surface(surface::Action),
}

impl cosmic::Application for CosmicBatteryApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletButton";

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        let config = Config::new(Self::APP_ID, BatteryAppletConfig::VERSION)
            .ok()
            .and_then(|c| BatteryAppletConfig::get_entry(&c).ok())
            .unwrap_or_default();

        let zbus_session_cmd = Task::perform(zbus::Connection::session(), |res| {
            cosmic::Action::App(Message::ZbusConnection(res))
        });
        let init_charging_limit_cmd = Task::perform(get_charging_limit(), |limit| {
            cosmic::Action::App(Message::InitChargingLimit(limit.ok()))
        });
        (
            Self {
                core,
                config,
                icon_name: "battery-symbolic".to_string(),
                display_icon_name: "display-brightness-symbolic".to_string(),
                token_tx: None,

                ..Default::default()
            },
            Task::batch([zbus_session_cmd, init_charging_limit_cmd]),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::Frame(now) => self.timeline.now(now),
            Message::SetKbdBrightness(brightness) => {
                self.kbd_brightness = Some(brightness);

                if !self.dragging_kbd_brightness {
                    self.dragging_kbd_brightness = true;
                    return cosmic::task::message(Message::SetKbdBrightnessDebounced);
                }
            }
            // Matching brightness calculation logic from cosmic-osd and cosmic-settings-daemon
            Message::SetScreenBrightness(brightness) => {
                let snapped = if let Some(max) = self.max_screen_brightness {
                    if max > 0 && max <= 20 {
                        // Coarse: map raw→k by round, then back to raw setpoint round(k*max/20)
                        let k = ((brightness as i64 * 20 + (max as i64) / 2) / (max as i64))
                            .clamp(0, 20);
                        (((k * (max as i64)) + 10) / 20) as i32
                    } else {
                        brightness
                    }
                } else {
                    brightness
                };
                self.screen_brightness = Some(snapped);
                if !self.dragging_screen_brightness {
                    self.dragging_screen_brightness = true;
                    self.update_display();
                    return cosmic::task::message(Message::SetScreenBrightnessDebounced);
                }
            }
            Message::SetKbdBrightnessDebounced => {
                if !self.dragging_kbd_brightness {
                    return Task::none();
                }
                if let Some(tx) = &self.kbd_sender {
                    if let Some(b) = self.kbd_brightness {
                        let _ = tx.send(KeyboardBacklightRequest::Set(b));
                    }
                }
                return cosmic::iced::Task::perform(
                    tokio::time::sleep(Duration::from_millis(200)),
                    |()| cosmic::Action::App(Message::SetKbdBrightnessDebounced),
                );
            }
            Message::SetScreenBrightnessDebounced => {
                if !self.dragging_screen_brightness {
                    return Task::none();
                }

                if let Some(tx) = &self.settings_daemon_sender {
                    if let Some(b) = self.screen_brightness {
                        let _ = tx.send(settings_daemon::Request::SetDisplayBrightness(b));
                    }
                }
                return cosmic::iced::Task::perform(
                    tokio::time::sleep(Duration::from_millis(200)),
                    |()| cosmic::Action::App(Message::SetScreenBrightnessDebounced),
                );
            }
            Message::ReleaseKbdBrightness => {
                self.dragging_kbd_brightness = false;
                if let Some(tx) = &self.kbd_sender {
                    if let Some(b) = self.kbd_brightness {
                        let _ = tx.send(KeyboardBacklightRequest::Set(b));
                    }
                }
            }
            Message::ReleaseScreenBrightness => {
                self.dragging_screen_brightness = false;

                self.update_display();
                if let Some(tx) = &self.settings_daemon_sender {
                    if let Some(b) = self.screen_brightness {
                        let _ = tx.send(settings_daemon::Request::SetDisplayBrightness(b));
                    }
                }
            }
            Message::InitChargingLimit(enable) => {
                if let Some(enable) = enable {
                    self.set_charging_limit(enable);
                }
            }
            Message::SetChargingLimit(chain, enable) => {
                self.timeline.set_chain(chain).start();
                self.set_charging_limit(enable);

                if enable {
                    return cosmic::iced::Task::perform(set_charging_limit(), |_| {
                        cosmic::Action::None
                    });
                } else {
                    return cosmic::iced::Task::perform(unset_charging_limit(), |_| {
                        cosmic::Action::None
                    });
                }
            }
            Message::Errored(why) => {
                tracing::error!("{}", why);
            }
            Message::TogglePopup => {
                self.dragging_kbd_brightness = false;
                self.dragging_screen_brightness = false;

                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    if let Some(tx) = &self.kbd_sender {
                        let _ = tx.send(KeyboardBacklightRequest::Get);
                    }
                    self.timeline = Timeline::new();

                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);

                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        Some((1, 1)),
                        None,
                        None,
                    );
                    if let Some(tx) = self.power_profile_sender.as_ref() {
                        let _ = tx.send(PowerProfileRequest::Get);
                    }
                    if let Some(tx) = self.update_trigger.as_ref() {
                        let _ = tx.send(());
                    }
                    let mut tasks = vec![get_popup(popup_settings)];
                    // Try again every time a popup is opened
                    if self.charging_limit.is_none() {
                        tasks.push(Task::perform(get_charging_limit(), |limit| {
                            cosmic::Action::App(Message::InitChargingLimit(limit.ok()))
                        }));
                    }
                    return Task::batch(tasks);
                }
            }
            Message::UpowerDevice(event) => match event {
                DeviceDbusEvent::Update {
                    on_battery,
                    percent,
                    time_to_empty,
                } => {
                    self.update_battery(percent, on_battery);
                    self.time_remaining = Duration::from_secs(time_to_empty as u64);
                }
                DeviceDbusEvent::NoBattery => {
                    std::process::exit(0);
                }
            },
            Message::KeyboardBacklight(event) => match event {
                KeyboardBacklightUpdate::Sender(tx) => {
                    self.kbd_sender = Some(tx);
                }
                KeyboardBacklightUpdate::MaxBrightness(max_brightness) => {
                    self.max_kbd_brightness = Some(max_brightness);
                }
                KeyboardBacklightUpdate::Brightness(brightness) => {
                    if !self.dragging_kbd_brightness {
                        self.kbd_brightness = Some(brightness);
                    }
                }
            },
            Message::InitProfile(tx, profile) => {
                self.power_profile_sender.replace(tx);
                self.power_profile = profile;
            }
            Message::Profile(profile) => {
                self.power_profile = profile;
                if let Some(tx) = &self.kbd_sender {
                    let _ = tx.send(KeyboardBacklightRequest::Get);
                }
            }
            Message::SelectProfile(profile) => {
                if let Some(tx) = self.power_profile_sender.as_ref() {
                    let _ = tx.send(PowerProfileRequest::Set(profile));
                }
            }
            Message::CloseRequested(id) => {
                self.dragging_kbd_brightness = false;
                self.dragging_screen_brightness = false;
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
            Message::ConfigChanged(config) => {
                self.config = config;
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
                    cmd.arg("power");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
            },
            Message::GpuInit(tx) => {
                self.update_trigger = Some(tx);
            }
            Message::GpuOn(path, name, app_list) => {
                let toggled = self.gpus.get(&path).is_some_and(|data| data.toggled);
                self.gpus.insert(
                    path,
                    GPUData {
                        name,
                        toggled,
                        app_list,
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
            Message::ZbusConnection(Err(err)) => {
                tracing::error!("Failed to connect to session dbus: {}", err);
            }
            Message::ZbusConnection(Ok(conn)) => {
                self.zbus_connection = Some(conn);
            }
            Message::SettingsDaemon(event) => match event {
                settings_daemon::Event::Sender(tx) => {
                    self.settings_daemon_sender = Some(tx);
                }
                settings_daemon::Event::MaxDisplayBrightness(max_brightness) => {
                    self.max_screen_brightness = Some(max_brightness);
                }
                settings_daemon::Event::DisplayBrightness(brightness) => {
                    if !self.dragging_screen_brightness {
                        self.screen_brightness = Some(brightness);
                    }
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

    fn view(&self) -> Element<Message> {
        let Spacing { space_xs, .. } = theme::active().cosmic().spacing;

        let is_horizontal = match self.core.applet.anchor {
            PanelAnchor::Top | PanelAnchor::Bottom => true,
            PanelAnchor::Left | PanelAnchor::Right => false,
        };

        let mut children = vec![icon::from_name(self.icon_name.as_str()).into()];

        let suggested_size = self.core.applet.suggested_size(true);
        let applet_padding = self.core.applet.suggested_padding(true);

        if self.config.show_percentage {
            children.push(
                self.core
                    .applet
                    .text(format!("{:.0}%", self.battery_percent))
                    .width(Length::Fixed(suggested_size.0 as f32))
                    .height(Length::Fixed(suggested_size.1 as f32))
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }

        let btn_content: Element<_> = if is_horizontal {
            row(children)
                .spacing(space_xs)
                .align_y(Alignment::Center)
                .into()
        } else {
            column(children)
                .spacing(space_xs)
                .align_x(Alignment::Center)
                .into()
        };

        let btn: Element<'_, Message> = button::custom(btn_content)
            .on_press_down(Message::TogglePopup)
            .class(Button::AppletIcon)
            .padding([applet_padding.0, applet_padding.1])
            .into();

        let content = if self.gpus.is_empty() {
            btn
        } else {
            let dot = container(vertical_space().height(Length::Fixed(0.0)))
                .padding(2.0)
                .class(cosmic::style::Container::Custom(Box::new(|theme| {
                    container::Style {
                        text_color: Some(Color::TRANSPARENT),
                        background: Some(Background::Color(theme.cosmic().accent_color().into())),
                        border: Border {
                            radius: 2.0.into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        shadow: Shadow::default(),
                        icon_color: Some(Color::TRANSPARENT),
                    }
                })));
            let (dot_align_x, dot_align_y) = match self.core.applet.anchor {
                PanelAnchor::Left => (Alignment::Start, Alignment::Center),
                PanelAnchor::Right => (Alignment::End, Alignment::Center),
                PanelAnchor::Top => (Alignment::Center, Alignment::Start),
                PanelAnchor::Bottom => (Alignment::Center, Alignment::End),
            };

            cosmic::iced::widget::stack![
                btn,
                container(dot)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(dot_align_y)
                    .align_x(dot_align_x)
                    .padding(2.0)
            ]
            .into()
        };

        self.core.applet.autosize_window(content).into()
    }

    fn view_window(&self, _id: window::Id) -> Element<'_, Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let name = text::body(fl!("battery"));
        let description = text::caption(
            if !self.on_battery || self.time_remaining == Duration::from_secs(0u64) {
                format!("{:.0}%", self.battery_percent)
            } else {
                format!(
                    "{} {} ({:.0}%)",
                    format_duration(self.time_remaining),
                    fl!("until-empty"),
                    self.battery_percent
                )
            },
        );

        let mut content = vec![
            padded_control(
                row![
                    icon::from_name(&*self.icon_name).size(24).symbolic(true),
                    column![name, description]
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .into(),
            padded_control(divider::horizontal::default())
                .padding([space_xxs, space_s])
                .into(),
            menu_button(
                row![
                    column![
                        text::body(fl!("battery")),
                        text::caption(fl!("battery-desc"))
                    ]
                    .width(Length::Fill),
                    if matches!(self.power_profile, Power::Battery) {
                        container(
                            icon::from_name("emblem-ok-symbolic")
                                .size(12)
                                .symbolic(true),
                        )
                    } else {
                        container(horizontal_space().width(1.0))
                    }
                ]
                .align_y(Alignment::Center),
            )
            .on_press(Message::SelectProfile(Power::Battery))
            .into(),
            menu_button(
                row![
                    column![
                        text::body(fl!("balanced")),
                        text::caption(fl!("balanced-desc"))
                    ]
                    .width(Length::Fill),
                    if matches!(self.power_profile, Power::Balanced) {
                        container(
                            icon::from_name("emblem-ok-symbolic")
                                .size(12)
                                .symbolic(true),
                        )
                    } else {
                        container(horizontal_space().width(1.0))
                    }
                ]
                .align_y(Alignment::Center),
            )
            .on_press(Message::SelectProfile(Power::Balanced))
            .into(),
            menu_button(
                row![
                    column![
                        text::body(fl!("performance")),
                        text::caption(fl!("performance-desc"))
                    ]
                    .width(Length::Fill),
                    if matches!(self.power_profile, Power::Performance) {
                        container(
                            icon::from_name("emblem-ok-symbolic")
                                .size(12)
                                .symbolic(true),
                        )
                    } else {
                        container(horizontal_space().width(1.0))
                    }
                ]
                .align_y(Alignment::Center),
            )
            .on_press(Message::SelectProfile(Power::Performance))
            .into(),
            padded_control(divider::horizontal::default())
                .padding([space_xxs, space_s])
                .into(),
        ];

        if let Some(charging_limit) = self.charging_limit {
            content.push(
                padded_control(
                    anim!(
                        //toggler
                        MAX_CHARGE,
                        &self.timeline,
                        fl!("max-charge"),
                        charging_limit,
                        Message::SetChargingLimit,
                    )
                    .text_size(14)
                    .width(Length::Fill),
                )
                .into(),
            );
            content.push(
                padded_control(divider::horizontal::default())
                    .padding([space_xxs, space_s])
                    .into(),
            );
        }

        if let Some(max_screen_brightness) = self.max_screen_brightness {
            if let Some(screen_brightness) = self.screen_brightness {
                content.push(
                    padded_control(
                        row![
                            icon::from_name(self.display_icon_name.as_str())
                                .size(24)
                                .symbolic(true),
                            slider(
                                0..=max_screen_brightness,
                                screen_brightness,
                                Message::SetScreenBrightness
                            )
                            .on_release(Message::ReleaseScreenBrightness),
                            container(
                                text(format!(
                                    "{:.0}%",
                                    self.screen_brightness_percent().unwrap_or(0.) * 100.
                                ))
                                .size(16)
                            )
                            .width(Length::Fixed(40.0))
                            .align_x(Alignment::End)
                        ]
                        .spacing(12),
                    )
                    .into(),
                );
            }
        }

        if let Some(max_kbd_brightness) = self.max_kbd_brightness {
            if let Some(kbd_brightness) = self.kbd_brightness {
                content.push(
                    padded_control(
                        row![
                            icon::from_name("keyboard-brightness-symbolic")
                                .size(24)
                                .symbolic(true),
                            slider(
                                0..=max_kbd_brightness,
                                kbd_brightness,
                                Message::SetKbdBrightness
                            )
                            .on_release(Message::ReleaseKbdBrightness),
                            container(
                                text(format!(
                                    "{:.0}%",
                                    100. * kbd_brightness as f64 / max_kbd_brightness as f64
                                ))
                                .size(16)
                            )
                            .width(Length::Fixed(40.0))
                            .align_x(Alignment::End)
                        ]
                        .spacing(12),
                    )
                    .into(),
                );
            }
        }

        content.push(
            padded_control(divider::horizontal::default())
                .padding([space_xxs, space_s])
                .into(),
        );

        if !self.gpus.is_empty() {
            content.push(
                padded_control(
                    row![
                        text(fl!("dgpu-running"))
                            .size(16)
                            .width(Length::Fill)
                            .align_x(Alignment::Start),
                        container(
                            vertical_space()
                                .width(Length::Fixed(0.0))
                                .height(Length::Fixed(0.0))
                        )
                        .padding(4)
                        .class(cosmic::style::Container::Custom(Box::new(|theme| {
                            container::Style {
                                text_color: Some(Color::TRANSPARENT),
                                background: Some(Background::Color(
                                    theme.cosmic().accent_color().into(),
                                )),
                                border: Border {
                                    radius: 4.0.into(),
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                },
                                shadow: Shadow::default(),
                                icon_color: Some(Color::TRANSPARENT),
                            }
                        },))),
                    ]
                    .align_y(Alignment::Center),
                )
                .into(),
            );
            content.push(
                padded_control(divider::horizontal::default())
                    .padding([space_xxs, space_s])
                    .into(),
            );
        }

        for (key, gpu) in &self.gpus {
            if gpu.app_list.is_none() {
                continue;
            }

            content.push(
                menu_button(
                    row![
                        text::body(fl!(
                            "dgpu-applications",
                            gpu_name = format!("\"{}\"", gpu.name.trim())
                        ))
                        .width(Length::Fill)
                        .align_y(Alignment::Center),
                        container(
                            icon::from_name(if gpu.toggled {
                                "go-down-symbolic"
                            } else {
                                "go-up-symbolic"
                            })
                            .size(14)
                            .symbolic(true)
                        )
                        .center(Length::Fixed(24.0)),
                    ]
                    .align_y(Alignment::Center),
                )
                .on_press(Message::ToggleGpuApps(key.clone()))
                .into(),
            );

            if gpu.toggled
                && !self.core.applet.suggested_bounds.as_ref().is_some_and(|c| {
                    let suggested_size = self.core.applet.suggested_size(true);
                    let padding = self.core.applet.suggested_padding(true).1;
                    let w = suggested_size.0 + 2 * padding;
                    let h = suggested_size.1 + 2 * padding;
                    // if we have a configure for width and height, we're in a overflow popup
                    // TODO... we don't exactly have a good way of knowing, unless the size is equal to a suggested size maybe?
                    c.width as u32 == w as u32 && c.height as u32 == h as u32
                })
            {
                let app_list = gpu.app_list.as_ref().unwrap();
                let mut list_apps = Vec::with_capacity(app_list.len());
                for app in app_list {
                    list_apps.push(
                        padded_control(
                            row![
                                if let Some(icon) = &app.icon {
                                    container(icon::from_name(&**icon).size(12).symbolic(true))
                                } else {
                                    container(horizontal_space().width(12.0))
                                },
                                column![text::body(&app.name), text::caption(&app.secondary)]
                                    .width(Length::Fill),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
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
            content.push(
                padded_control(divider::horizontal::default())
                    .padding([space_xxs, space_s])
                    .into(),
            );
        }

        content.push(
            menu_button(text::body(fl!("power-settings")).width(Length::Fill))
                .on_press(Message::OpenSettings)
                .into(),
        );

        self.core
            .applet
            .popup_container(Column::with_children(content).padding([8, 0]))
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            device_subscription(0).map(Message::UpowerDevice),
            kbd_backlight_subscription(0).map(Message::KeyboardBacklight),
            power_profile_subscription(0).map(|event| match event {
                PowerProfileUpdate::Update { profile } => Message::Profile(profile),
                PowerProfileUpdate::Init(tx, p) => Message::InitProfile(p, tx),
                PowerProfileUpdate::Error(e) => Message::Errored(e), // TODO: handle error
            }),
            dgpu_subscription(0).map(|event| match event {
                GpuUpdate::Init(tx) => Message::GpuInit(tx),
                GpuUpdate::On(path, name, list) => Message::GpuOn(path, name, list),
                GpuUpdate::Off(path) => Message::GpuOff(path),
            }),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
            activation_token_subscription(0).map(Message::Token),
            self.core.watch_config(Self::APP_ID).map(|u| {
                for err in u.errors {
                    tracing::error!(?err, "Error watching config");
                }
                Message::ConfigChanged(u.config)
            }),
        ];
        if let Some(conn) = self.zbus_connection.clone() {
            subscriptions.push(settings_daemon::subscription(conn).map(Message::SettingsDaemon));
        }
        Subscription::batch(subscriptions)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
