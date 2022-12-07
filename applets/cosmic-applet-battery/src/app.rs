use crate::backlight::{
    screen_backlight_subscription, ScreenBacklightRequest, ScreenBacklightUpdate,
};
use crate::config;
use crate::fl;
use crate::upower_device::{device_subscription, DeviceDbusEvent};
use crate::upower_kbdbacklight::{
    kbd_backlight_subscription, KeyboardBacklightRequest, KeyboardBacklightUpdate,
};
use cosmic::applet::CosmicAppletHelper;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::{
    executor,
    widget::{button, column, row, text, slider},
    window, Alignment, Application, Command, Length, Subscription,
};
use cosmic::iced_native::window::Settings;
use cosmic::iced_style::application::{self, Appearance};
use cosmic::iced_style::svg;
use cosmic::theme::{self, Svg};
use cosmic::widget::{icon, toggler, horizontal_rule};
use cosmic::{iced_style, settings, Element, Theme};
use cosmic_panel_config::{PanelAnchor, PanelSize};
use iced_sctk::application::SurfaceIdWrapper;
use iced_sctk::command::platform_specific::wayland::window::SctkWindowSettings;
use iced_sctk::commands::popup::{destroy_popup, get_popup};
use iced_sctk::settings::InitialSurface;
use iced_sctk::Color;
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
    let mut settings = settings();
    let helper = CosmicAppletHelper::default();
    let pixels = helper.suggested_icon_size() as u32;
    settings.initial_surface = InitialSurface::XdgWindow(SctkWindowSettings {
        iced_settings: Settings {
            size: (pixels + 16, pixels + 16),
            min_size: Some((pixels + 16, pixels + 16)),
            max_size: Some((pixels + 16, pixels + 16)),
            ..Default::default()
        },
        ..Default::default()
    });
    CosmicBatteryApplet::run(settings)
}

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
    id_ctr: u32,
    screen_sender: Option<UnboundedSender<ScreenBacklightRequest>>,
    kbd_sender: Option<UnboundedSender<KeyboardBacklightRequest>>,
    applet_helper: CosmicAppletHelper,
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
    SetChargingLimit(bool),
    UpdateKbdBrightness(f64),
    UpdateScreenBrightness(f64),
    OpenBatterySettings,
    InitKbdBacklight(UnboundedSender<KeyboardBacklightRequest>, f64),
    InitScreenBacklight(UnboundedSender<ScreenBacklightRequest>, f64),
    Errored(String),
    Ignore,
}

impl Application for CosmicBatteryApplet {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            CosmicBatteryApplet {
                icon_name: "battery-symbolic".to_string(),
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
            Message::SetKbdBrightness(brightness) => {
                self.kbd_brightness = (brightness as f64 / 100.0).clamp(0., 1.);
                if let Some(tx) = &self.kbd_sender {
                    let _ = tx.send(KeyboardBacklightRequest::Set(self.kbd_brightness));
                }
            }
            Message::SetScreenBrightness(brightness) => {
                self.screen_brightness = brightness as f64 / 100.0;
                if let Some(tx) = &self.screen_sender {
                    let _ = tx.send(ScreenBacklightRequest::Set(self.screen_brightness));
                }
            }
            Message::SetChargingLimit(enable_charging_limit) => {
                self.charging_limit = enable_charging_limit;
            }
            Message::OpenBatterySettings => {
                // TODO Ashley
            }
            Message::Errored(_) => {
                // TODO log errors
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
                    let new_id = window::Id::new(self.id_ctr);
                    self.popup.replace(new_id);

                    let popup_settings =
                        self.applet_helper.get_popup_settings(window::Id::new(0), new_id, (400, 240), None, None);
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
        }
        Command::none()
    }
    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => self.applet_helper.icon_button(
                &self.icon_name,
            )
            .on_press(Message::TogglePopup)
            .into(),
            SurfaceIdWrapper::Popup(_) => {
                let name = text(fl!("battery")).size(18);
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
                .size(12);
                self.applet_helper.popup_container(
                    column![
                        row![
                            icon(&self.icon_name, 24)
                                .style(Svg::Custom(|theme| {
                                    svg::Appearance {
                                        fill: Some(theme.palette().text),
                                    }
                                }))
                                .width(Length::Units(24))
                                .height(Length::Units(24)),
                            column![name, description]
                        ]
                        .spacing(8)
                        .align_items(Alignment::Center),
                        horizontal_rule(1),
                        // text{"Limit Battery Charging"},
                        toggler(fl!("max-charge"), self.charging_limit, |_| {
                            Message::SetChargingLimit(!self.charging_limit)
                        }),
                        horizontal_rule(1),
                        row![
                            icon("display-brightness-symbolic", 24)
                                .style(Svg::Custom(|theme| {
                                    svg::Appearance {
                                        fill: Some(theme.palette().text),
                                    }
                                }))
                                .width(Length::Units(24))
                                .height(Length::Units(24)),
                            slider(
                                0..=100,
                                (self.screen_brightness * 100.0) as i32,
                                Message::SetScreenBrightness
                            ),
                            text(format!("{:.0}%", self.screen_brightness * 100.0))
                                .width(Length::Units(40))
                                .horizontal_alignment(Horizontal::Right)
                        ]
                        .spacing(12),
                        row![
                            icon("keyboard-brightness-symbolic", 24)
                                .style(Svg::Custom(|theme| {
                                    svg::Appearance {
                                        fill: Some(theme.palette().text),
                                    }
                                }))
                                .width(Length::Units(24))
                                .height(Length::Units(24)),
                            slider(
                                0..=100,
                                (self.kbd_brightness * 100.0) as i32,
                                Message::SetKbdBrightness
                            ),
                            text(format!("{:.0}%", self.kbd_brightness * 100.0))
                                .width(Length::Units(40))
                                .horizontal_alignment(Horizontal::Right)
                        ]
                        .spacing(12),
                        button(
                            text(fl!("power-settings"))
                                .horizontal_alignment(Horizontal::Center)
                                .width(Length::Fill)
                                .style(theme::Text::Custom(|theme| {
                                    let cosmic = theme.cosmic();
                                    iced_style::text::Appearance {
                                        color: Some(cosmic.accent.on.into()),
                                    }
                                }))
                        )
                        .width(Length::Fill)
                    ]
                    .spacing(4)
                    .padding(8),
                )
                .into()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            device_subscription(0).map(|(_, event)| match event {
                DeviceDbusEvent::Update {
                    icon_name,
                    percent,
                    time_to_empty,
                } => Message::Update {
                    icon_name,
                    percent,
                    time_to_empty,
                },
            }),
            kbd_backlight_subscription(0).map(|(_, event)| match event {
                KeyboardBacklightUpdate::Update(b) => Message::UpdateKbdBrightness(b),
                KeyboardBacklightUpdate::Init(tx, b) => Message::InitKbdBacklight(tx, b),
            }),
            screen_backlight_subscription(0).map(|(_, event)| match event {
                ScreenBacklightUpdate::Update(b) => Message::UpdateScreenBrightness(b),
                ScreenBacklightUpdate::Init(tx, b) => Message::InitScreenBacklight(tx, b),
            }),
        ])
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
}
