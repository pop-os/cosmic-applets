// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod localize;
mod mouse_area;

use std::time::Duration;

use crate::{localize::localize, pulse::DeviceInfo};
use config::AudioAppletConfig;
use cosmic::{
    app::Command,
    applet::{
        cosmic_panel_config::PanelAnchor,
        menu_button, menu_control_padding, padded_control,
        token::subscription::{activation_token_subscription, TokenRequest, TokenUpdate},
    },
    cctk::sctk::reexports::{calloop, protocols::xdg::shell::client::xdg_positioner::Anchor},
    cosmic_config::CosmicConfigEntry,
    iced::{
        self,
        widget::{self, column, row, slider},
        window, Alignment, Length, Limits, Rectangle, Subscription,
    },
    iced_runtime::core::alignment::Horizontal,
    iced_style::application,
    widget::{button, divider, horizontal_space, icon, text, Column, Row},
    Element, Renderer, Theme,
};
use cosmic_settings_subscriptions::pulse as sub_pulse;
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};
use iced::{
    wayland::popup::{destroy_popup, get_popup},
    widget::container,
};
use libpulse_binding::volume::Volume;
use mpris2_zbus::player::PlaybackStatus;
use mpris_subscription::{MprisRequest, MprisUpdate};

mod config;
mod mpris_subscription;
mod pulse;

static SHOW_MEDIA_CONTROLS: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

const GO_BACK: &str = "media-skip-backward-symbolic";
const GO_NEXT: &str = "media-skip-forward-symbolic";
const PAUSE: &str = "media-playback-pause-symbolic";
const PLAY: &str = "media-playback-start-symbolic";

pub fn run() -> cosmic::iced::Result {
    localize();
    cosmic::applet::run::<Audio>(true, ())
}

#[derive(Default)]
pub struct Audio {
    core: cosmic::app::Core,
    is_open: IsOpen,
    output_volume: f64,
    output_volume_debounce: bool,
    output_volume_text: String,
    input_volume: f64,
    input_volume_debounce: bool,
    input_volume_text: String,
    current_output: Option<DeviceInfo>,
    current_input: Option<DeviceInfo>,
    outputs: Vec<DeviceInfo>,
    inputs: Vec<DeviceInfo>,
    pulse_state: PulseState,
    popup: Option<window::Id>,
    timeline: Timeline,
    config: AudioAppletConfig,
    player_status: Option<mpris_subscription::PlayerStatus>,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
}

impl Audio {
    fn update_output(&mut self, output: Option<DeviceInfo>) {
        self.current_output = output;

        if let Some(device) = self.current_output.as_ref() {
            self.output_volume = volume_to_percent(device.volume.avg());
            self.output_volume_text = format!("{}%", self.output_volume.round());
        }
    }

    fn output_icon_name(&self) -> &'static str {
        let volume = self.output_volume;
        let mute = self.current_output_mute();
        if mute || volume == 0. {
            "audio-volume-muted-symbolic"
        } else if volume < 33. {
            "audio-volume-low-symbolic"
        } else if volume < 66. {
            "audio-volume-medium-symbolic"
        } else if volume <= 100. {
            "audio-volume-high-symbolic"
        } else {
            "audio-volume-overamplified-symbolic"
        }
    }

    fn update_input(&mut self, input: Option<DeviceInfo>) {
        self.current_input = input;

        if let Some(device) = self.current_output.as_ref() {
            self.input_volume = volume_to_percent(device.volume.avg());
            self.input_volume_text = format!("{}%", self.input_volume.round());
        }
    }

    fn input_icon_name(&self) -> &'static str {
        let volume = self.input_volume;
        let mute = self.current_input_mute();
        if mute || volume == 0. {
            "microphone-sensitivity-muted-symbolic"
        } else if volume < 33. {
            "microphone-sensitivity-low-symbolic"
        } else if volume < 66. {
            "microphone-sensitivity-medium-symbolic"
        } else {
            "microphone-sensitivity-high-symbolic"
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum IsOpen {
    None,
    Output,
    Input,
}

#[derive(Debug, Clone)]
pub enum Message {
    Ignore,
    ApplyOutputVolume,
    ApplyInputVolume,
    SetOutputVolume(f64),
    SetInputVolume(f64),
    SetOutputMute(bool),
    SetInputMute(bool),
    OutputToggle,
    InputToggle,
    OutputChanged(String),
    InputChanged(String),
    Pulse(pulse::Event),
    TogglePopup,
    CloseRequested(window::Id),
    ToggleMediaControlsInTopPanel(chain::Toggler, bool),
    Frame(Instant),
    ConfigChanged(AudioAppletConfig),
    Mpris(mpris_subscription::MprisUpdate),
    MprisRequest(MprisRequest),
    Token(TokenUpdate),
    OpenSettings,
    PulseSub(sub_pulse::Event),
}

impl Audio {
    fn playback_buttons(&self) -> Option<Element<Message>> {
        if self.player_status.is_some() && self.config.show_media_controls_in_top_panel {
            let mut elements = Vec::with_capacity(3);
            if self
                .player_status
                .as_ref()
                .map(|s| s.can_go_previous)
                .unwrap_or_default()
            {
                elements.push(
                    self.core
                        .applet
                        .icon_button(GO_BACK)
                        .on_press(Message::MprisRequest(MprisRequest::Previous))
                        .into(),
                )
            }
            if let Some(play) = self.is_play() {
                elements.push(
                    self.core
                        .applet
                        .icon_button(if play { PLAY } else { PAUSE })
                        .on_press(if play {
                            Message::MprisRequest(MprisRequest::Play)
                        } else {
                            Message::MprisRequest(MprisRequest::Pause)
                        })
                        .into(),
                );
            }
            if self
                .player_status
                .as_ref()
                .map(|s| s.can_go_next)
                .unwrap_or_default()
            {
                elements.push(
                    self.core
                        .applet
                        .icon_button(GO_NEXT)
                        .on_press(Message::MprisRequest(MprisRequest::Next))
                        .into(),
                )
            }

            Some(match self.core.applet.anchor {
                PanelAnchor::Left | PanelAnchor::Right => Column::with_children(elements)
                    .align_items(Alignment::Center)
                    .into(),
                PanelAnchor::Top | PanelAnchor::Bottom => Row::with_children(elements)
                    .align_items(Alignment::Center)
                    .into(),
            })
        } else {
            None
        }
    }

    fn go_previous(&self, icon_size: u16) -> Option<Element<Message>> {
        self.player_status.as_ref().and_then(|s| {
            if s.can_go_previous {
                Some(
                    button::icon(icon::from_name(GO_BACK).size(icon_size).symbolic(true))
                        .extra_small()
                        .style(cosmic::theme::Button::AppletIcon)
                        .on_press(Message::MprisRequest(MprisRequest::Previous))
                        .into(),
                )
            } else {
                None
            }
        })
    }

    fn go_next(&self, icon_size: u16) -> Option<Element<Message>> {
        self.player_status.as_ref().and_then(|s| {
            if s.can_go_next {
                Some(
                    button::icon(icon::from_name(GO_NEXT).size(icon_size).symbolic(true))
                        .extra_small()
                        .style(cosmic::theme::Button::AppletIcon)
                        .on_press(Message::MprisRequest(MprisRequest::Next))
                        .into(),
                )
            } else {
                None
            }
        })
    }

    fn is_play(&self) -> Option<bool> {
        self.player_status.as_ref().and_then(|s| match s.status {
            PlaybackStatus::Playing => {
                if s.can_pause {
                    Some(false)
                } else {
                    None
                }
            }

            PlaybackStatus::Paused | PlaybackStatus::Stopped => {
                if s.can_play {
                    Some(true)
                } else {
                    None
                }
            }
        })
    }

    fn current_output_mute(&self) -> bool {
        self.current_output
            .as_ref()
            .map(|o| o.mute)
            .unwrap_or_default()
    }

    fn current_input_mute(&self) -> bool {
        self.current_input
            .as_ref()
            .map(|o| o.mute)
            .unwrap_or_default()
    }
}

impl cosmic::Application for Audio {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletAudio";

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                core,
                is_open: IsOpen::None,
                current_output: None,
                current_input: None,
                outputs: vec![],
                inputs: vec![],
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

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Frame(now) => self.timeline.now(now),
            Message::Ignore => {}
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::UpdateConnection);
                    }
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);
                    self.timeline = Timeline::new();

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        window::Id::MAIN,
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .min_height(1.0)
                        .min_width(1.0)
                        .max_width(400.0)
                        .max_height(1080.0);

                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::GetDefaultSink);
                        conn.send(pulse::Message::GetDefaultSource);
                        conn.send(pulse::Message::GetSinks);
                        conn.send(pulse::Message::GetSources);
                    }

                    return get_popup(popup_settings);
                }
            }
            Message::SetOutputVolume(vol) => {
                if self.output_volume == vol {
                    return Command::none();
                }

                self.output_volume = vol;
                self.output_volume_text = format!("{}%", self.output_volume.round());

                if self.output_volume_debounce {
                    return Command::none();
                }

                self.output_volume_debounce = true;

                return cosmic::command::future(async move {
                    tokio::time::sleep(Duration::from_millis(64)).await;
                    Message::ApplyOutputVolume
                });
            }
            Message::SetInputVolume(vol) => {
                if self.input_volume == vol {
                    return Command::none();
                }

                self.input_volume = vol;
                self.input_volume_text = format!("{}%", self.input_volume.round());

                if self.input_volume_debounce {
                    return Command::none();
                }

                self.input_volume_debounce = true;

                return cosmic::command::future(async move {
                    tokio::time::sleep(Duration::from_millis(64)).await;
                    Message::ApplyInputVolume
                });
            }
            Message::ApplyOutputVolume => {
                self.output_volume_debounce = false;

                self.current_output.as_mut().map(|o| {
                    o.volume
                        .set(o.volume.len(), percent_to_volume(self.output_volume))
                });

                if let PulseState::Connected(connection) = &mut self.pulse_state {
                    if let Some(device) = &self.current_output {
                        if let Some(name) = &device.name {
                            connection.send(pulse::Message::SetSinkVolumeByName(
                                name.clone(),
                                device.volume,
                            ))
                        }
                    }
                }
            }
            Message::ApplyInputVolume => {
                self.input_volume_debounce = false;

                self.current_input.as_mut().map(|i| {
                    i.volume
                        .set(i.volume.len(), percent_to_volume(self.input_volume))
                });

                if let PulseState::Connected(connection) = &mut self.pulse_state {
                    if let Some(device) = &self.current_input {
                        if let Some(name) = &device.name {
                            tracing::info!("increasing volume of {}", name);
                            connection.send(pulse::Message::SetSourceVolumeByName(
                                name.clone(),
                                device.volume,
                            ))
                        }
                    }
                }
            }
            Message::SetOutputMute(mute) => {
                if let Some(output) = self.current_output.as_mut() {
                    output.mute = mute;
                }
                if let PulseState::Connected(connection) = &mut self.pulse_state {
                    if let Some(device) = &self.current_output {
                        if let Some(name) = &device.name {
                            connection
                                .send(pulse::Message::SetSinkMuteByName(name.clone(), device.mute))
                        }
                    }
                }
            }
            Message::SetInputMute(mute) => {
                if let Some(input) = self.current_input.as_mut() {
                    input.mute = mute;
                }
                if let PulseState::Connected(connection) = &mut self.pulse_state {
                    if let Some(device) = &self.current_input {
                        if let Some(name) = &device.name {
                            connection.send(pulse::Message::SetSourceMuteByName(
                                name.clone(),
                                device.mute,
                            ))
                        }
                    }
                }
            }
            Message::OutputChanged(val) => {
                if let Some(conn) = self.pulse_state.connection() {
                    if let Some(val) = self.outputs.iter().find(|o| o.name.as_ref() == Some(&val)) {
                        conn.send(pulse::Message::SetDefaultSink(val.clone()));
                    }
                }
            }
            Message::InputChanged(val) => {
                if let Some(conn) = self.pulse_state.connection() {
                    if let Some(val) = self.inputs.iter().find(|i| i.name.as_ref() == Some(&val)) {
                        conn.send(pulse::Message::SetDefaultSource(val.clone()));
                    }
                }
            }
            Message::OutputToggle => {
                self.is_open = if self.is_open == IsOpen::Output {
                    IsOpen::None
                } else {
                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::GetSinks);
                    }
                    IsOpen::Output
                }
            }
            Message::InputToggle => {
                self.is_open = if self.is_open == IsOpen::Input {
                    IsOpen::None
                } else {
                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::GetSources);
                    }
                    IsOpen::Input
                }
            }
            Message::Pulse(event) => match event {
                pulse::Event::Init(mut conn) => {
                    conn.send(pulse::Message::UpdateConnection);
                    self.pulse_state = PulseState::Disconnected(conn);
                }
                pulse::Event::Connected => {
                    self.pulse_state.connected();

                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::GetSinks);
                        conn.send(pulse::Message::GetSources);
                        conn.send(pulse::Message::GetDefaultSink);
                        conn.send(pulse::Message::GetDefaultSource);
                    }
                }
                pulse::Event::MessageReceived(msg) => {
                    match msg {
                        // This is where we match messages from the subscription to app state
                        pulse::Message::SetSinks(sinks) => self.outputs = sinks,
                        pulse::Message::SetSources(sources) => {
                            self.inputs = sources
                                .into_iter()
                                .filter(|source| {
                                    !source
                                        .name
                                        .as_ref()
                                        .unwrap_or(&String::from("Generic"))
                                        .contains("monitor")
                                })
                                .collect()
                        }
                        pulse::Message::SetDefaultSink(sink) => {
                            self.update_output(Some(sink));
                        }
                        pulse::Message::SetDefaultSource(source) => {
                            self.update_input(Some(source));
                        }
                        pulse::Message::Disconnected => {
                            panic!("Subscription error handling is bad. This should never happen.")
                        }
                        _ => {
                            tracing::trace!("Received misc message")
                        }
                    }
                }
                pulse::Event::Disconnected => {
                    self.pulse_state.disconnected();
                    if let Some(mut conn) = self.pulse_state.connection().cloned() {
                        _ = tokio::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                            conn.send(pulse::Message::UpdateConnection);
                        });
                    }
                }
            },
            Message::ToggleMediaControlsInTopPanel(chain, enabled) => {
                self.timeline.set_chain(chain).start();
                self.config.show_media_controls_in_top_panel = enabled;
                if let Ok(helper) =
                    cosmic::cosmic_config::Config::new(Self::APP_ID, AudioAppletConfig::VERSION)
                {
                    if let Err(err) = self.config.write_entry(&helper) {
                        tracing::error!(?err, "Error writing config");
                    }
                }
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
            Message::ConfigChanged(c) => {
                self.config = c;
            }
            Message::Mpris(mpris_subscription::MprisUpdate::Player(p)) => {
                self.player_status = Some(p);
            }
            Message::Mpris(MprisUpdate::Finished) => {
                self.player_status = None;
            }
            Message::Mpris(MprisUpdate::Setup) => {
                self.player_status = None;
            }
            Message::MprisRequest(r) => {
                let Some(player_status) = self.player_status.as_ref() else {
                    tracing::error!("No player found");
                    return Command::none();
                };
                let player = player_status.player.clone();

                match r {
                    MprisRequest::Play => tokio::spawn(async move {
                        let res = player.play().await;
                        if let Err(err) = res {
                            tracing::error!("Error playing: {}", err);
                        }
                    }),
                    MprisRequest::Pause => tokio::spawn(async move {
                        let res = player.pause().await;
                        if let Err(err) = res {
                            tracing::error!("Error pausing: {}", err);
                        }
                    }),
                    MprisRequest::Next => tokio::spawn(async move {
                        let res = player.next().await;
                        if let Err(err) = res {
                            tracing::error!("Error playing next: {}", err);
                        }
                    }),
                    MprisRequest::Previous => tokio::spawn(async move {
                        let res = player.previous().await;
                        if let Err(err) = res {
                            tracing::error!("Error playing previous: {}", err);
                        }
                    }),
                };
            }
            Message::OpenSettings => {
                let exec = "cosmic-settings sound".to_string();
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
                    cmd.arg("sound");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
            },
            Message::PulseSub(event) => match event {
                sub_pulse::Event::SinkVolume(value) => {
                    self.current_output.as_mut().map(|output| {
                        output
                            .volume
                            .set(output.volume.len(), percent_to_volume(value as f64))
                    });
                }
                sub_pulse::Event::SinkMute(value) => {
                    if let Some(output) = self.current_output.as_mut() {
                        output.mute = value;
                    }
                }
                sub_pulse::Event::SourceVolume(value) => {
                    self.current_input.as_mut().map(|input| {
                        input
                            .volume
                            .set(input.volume.len(), percent_to_volume(value as f64))
                    });
                }
                sub_pulse::Event::SourceMute(value) => {
                    if let Some(input) = self.current_input.as_mut() {
                        input.mute = value;
                    }
                }
            },
        };

        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            pulse::connect().map(Message::Pulse),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
            self.core.watch_config(Self::APP_ID).map(|u| {
                for err in u.errors {
                    tracing::error!(?err, "Error watching config");
                }
                Message::ConfigChanged(u.config)
            }),
            mpris_subscription::mpris_subscription(0).map(Message::Mpris),
            activation_token_subscription(0).map(Message::Token),
            sub_pulse::subscription().map(Message::PulseSub),
        ])
    }

    fn view(&self) -> Element<Message> {
        let btn = self
            .core
            .applet
            .icon_button(self.output_icon_name())
            .on_press_down(Message::TogglePopup);
        let btn = crate::mouse_area::MouseArea::new(btn).on_mouse_wheel(|delta| {
            let change = match delta {
                iced::mouse::ScrollDelta::Lines { x, y } => (x + y) * 5.,
                iced::mouse::ScrollDelta::Pixels { y, .. } => y / 40.3125,
            };
            if change.abs() < f32::EPSILON {
                return Message::Ignore;
            }
            let new_volume = self
                .current_output
                .as_ref()
                .map_or(0f64, |v| volume_to_percent(v.volume.avg()) + change as f64)
                .clamp(0.0, 100.0);
            Message::SetOutputVolume(new_volume)
        });
        let playback_buttons = (!self.core.applet.configure.as_ref().is_some_and(|c| {
            // if we have a configure for width and height, we're in a overflow popup
            c.new_size.0.is_some() && c.new_size.1.is_some()
        }))
        .then(|| self.playback_buttons());

        if let Some(Some(playback_buttons)) = playback_buttons {
            match self.core.applet.anchor {
                PanelAnchor::Left | PanelAnchor::Right => {
                    Column::with_children(vec![playback_buttons, btn.into()])
                        .align_items(Alignment::Center)
                        .into()
                }
                PanelAnchor::Top | PanelAnchor::Bottom => {
                    Row::with_children(vec![playback_buttons, btn.into()])
                        .align_items(Alignment::Center)
                        .into()
                }
            }
        } else {
            btn.into()
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let audio_disabled = matches!(self.pulse_state, PulseState::Disconnected(_));
        let out_mute = self.current_output_mute();
        let in_mute = self.current_input_mute();

        let mut audio_content = if audio_disabled {
            column![padded_control(
                text(fl!("disconnected"))
                    .width(Length::Fill)
                    .horizontal_alignment(Horizontal::Center)
                    .size(24)
            )]
        } else {
            column![
                padded_control(
                    row![
                        button::icon(
                            icon::from_name(self.output_icon_name())
                                .size(24)
                                .symbolic(true),
                        )
                        .style(cosmic::theme::Button::Icon)
                        .icon_size(24)
                        .line_height(24)
                        .on_press(Message::SetOutputMute(!out_mute)),
                        slider(0.0..=100.0, self.output_volume, Message::SetOutputVolume)
                            .width(Length::FillPortion(5)),
                        text(&self.output_volume_text)
                            .size(16)
                            .width(Length::FillPortion(1))
                            .horizontal_alignment(Horizontal::Right)
                    ]
                    .spacing(12)
                    .align_items(Alignment::Center)
                ),
                padded_control(
                    row![
                        button::icon(
                            icon::from_name(self.input_icon_name())
                                .size(24)
                                .symbolic(true),
                        )
                        .style(cosmic::theme::Button::Icon)
                        .icon_size(24)
                        .line_height(24)
                        .on_press(Message::SetInputMute(!in_mute)),
                        slider(0.0..=100.0, self.input_volume, Message::SetInputVolume)
                            .width(Length::FillPortion(5)),
                        text(&self.input_volume_text)
                            .size(16)
                            .width(Length::FillPortion(1))
                            .horizontal_alignment(Horizontal::Right)
                    ]
                    .spacing(12)
                    .align_items(Alignment::Center)
                ),
                padded_control(divider::horizontal::default()),
                revealer(
                    self.is_open == IsOpen::Output,
                    fl!("output"),
                    match &self.current_output {
                        Some(output) => pretty_name(output.description.clone()),
                        None => String::from("No device selected"),
                    },
                    self.outputs
                        .clone()
                        .into_iter()
                        .map(|output| (
                            output.name.clone().unwrap_or_default(),
                            pretty_name(output.description)
                        ))
                        .collect(),
                    Message::OutputToggle,
                    Message::OutputChanged,
                ),
                revealer(
                    self.is_open == IsOpen::Input,
                    fl!("input"),
                    match &self.current_input {
                        Some(input) => pretty_name(input.description.clone()),
                        None => fl!("no-device"),
                    },
                    self.inputs
                        .clone()
                        .into_iter()
                        .map(|input| (
                            input.name.clone().unwrap_or_default(),
                            pretty_name(input.description)
                        ))
                        .collect(),
                    Message::InputToggle,
                    Message::InputChanged,
                )
            ]
            .align_items(Alignment::Start)
        };

        if let Some(s) = self.player_status.as_ref() {
            let mut elements = Vec::with_capacity(5);

            if let Some(icon_path) = s.icon.clone() {
                elements.push(icon(icon::from_path(icon_path)).size(36).into());
            }

            let title = if let Some(title) = s.title.as_ref() {
                if title.chars().count() > 15 {
                    let mut title_trunc = title.chars().take(15).collect::<String>();
                    title_trunc.push_str("...");
                    title_trunc
                } else {
                    title.to_string()
                }
            } else {
                String::new()
            };

            let artists = if let Some(artists) = s.artists.as_ref() {
                let artists = artists.join(", ");
                if artists.chars().count() > 15 {
                    let mut artists_trunc = artists.chars().take(15).collect::<String>();
                    artists_trunc.push_str("...");
                    artists_trunc
                } else {
                    artists
                }
            } else {
                fl!("unknown-artist")
            };

            elements.push(
                column![
                    text::body(title).width(Length::Shrink),
                    text::caption(artists).width(Length::Shrink),
                ]
                .width(Length::FillPortion(5))
                .into(),
            );

            let mut control_elements = Vec::with_capacity(4);
            control_elements.push(horizontal_space(Length::Fill).into());
            if let Some(go_prev) = self.go_previous(32) {
                control_elements.push(go_prev);
            }
            if let Some(play) = self.is_play() {
                control_elements.push(
                    button::icon(
                        icon::from_name(if play { PLAY } else { PAUSE })
                            .size(32)
                            .symbolic(true),
                    )
                    .extra_small()
                    .style(cosmic::theme::Button::AppletIcon)
                    .on_press(if play {
                        Message::MprisRequest(MprisRequest::Play)
                    } else {
                        Message::MprisRequest(MprisRequest::Pause)
                    })
                    .into(),
                );
            }
            if let Some(go_next) = self.go_next(32) {
                control_elements.push(go_next);
            }
            let control_cnt = control_elements.len() as u16;
            elements.push(
                Row::with_children(control_elements)
                    .align_items(Alignment::Center)
                    .width(Length::FillPortion(control_cnt.saturating_add(1)))
                    .spacing(8)
                    .into(),
            );

            audio_content = audio_content.push(padded_control(divider::horizontal::default()));
            audio_content = audio_content.push(
                Row::with_children(elements)
                    .align_items(Alignment::Center)
                    .spacing(8)
                    .padding(menu_control_padding()),
            );
        }
        let content = column![
            audio_content,
            padded_control(divider::horizontal::default()),
            padded_control(
                anim!(
                    // toggler
                    SHOW_MEDIA_CONTROLS,
                    &self.timeline,
                    Some(fl!("show-media-controls")),
                    self.config.show_media_controls_in_top_panel,
                    Message::ToggleMediaControlsInTopPanel,
                )
                .text_size(14)
                .width(Length::Fill)
            )
            .padding([8, 24]),
            padded_control(divider::horizontal::default()),
            menu_button(text::body(fl!("sound-settings"))).on_press(Message::OpenSettings)
        ]
        .align_items(Alignment::Start)
        .padding([8, 0]);

        self.core.applet.popup_container(container(content)).into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}

fn revealer(
    open: bool,
    title: String,
    selected: String,
    options: Vec<(String, String)>,
    toggle: Message,
    mut change: impl FnMut(String) -> Message + 'static,
) -> widget::Column<'static, Message, crate::Theme, Renderer> {
    if open {
        options.iter().fold(
            column![revealer_head(open, title, selected, toggle)].width(Length::Fill),
            |col, (id, name)| {
                col.push(
                    menu_button(text::body(name.clone()))
                        .on_press(change(id.clone()))
                        .width(Length::Fill)
                        .padding([8, 48]),
                )
            },
        )
    } else {
        column![revealer_head(open, title, selected, toggle)]
    }
}

fn revealer_head(
    _open: bool,
    title: String,
    selected: String,
    toggle: Message,
) -> cosmic::widget::Button<'static, Message> {
    menu_button(column![
        text::body(title).width(Length::Fill),
        text::caption(selected),
    ])
    .on_press(toggle)
}

fn pretty_name(name: Option<String>) -> String {
    match name {
        Some(n) => n,
        None => String::from("Generic"),
    }
}

#[derive(Default)]
enum PulseState {
    #[default]
    Init,
    Disconnected(pulse::Connection),
    Connected(pulse::Connection),
}

impl PulseState {
    fn connection(&mut self) -> Option<&mut pulse::Connection> {
        match self {
            Self::Disconnected(c) => Some(c),
            Self::Connected(c) => Some(c),
            Self::Init => None,
        }
    }

    fn connected(&mut self) {
        if let Self::Disconnected(c) = self {
            *self = Self::Connected(c.clone());
        }
    }

    fn disconnected(&mut self) {
        if let Self::Connected(c) = self {
            *self = Self::Disconnected(c.clone());
        }
    }
}

impl Default for IsOpen {
    fn default() -> Self {
        Self::None
    }
}

fn volume_to_percent(volume: Volume) -> f64 {
    volume.0 as f64 * 100. / Volume::NORMAL.0 as f64
}

fn percent_to_volume(percent: f64) -> Volume {
    Volume(
        (percent / 100. * Volume::NORMAL.0 as f64)
            .clamp(0., Volume::NORMAL.0 as f64)
            .round() as u32,
    )
}
