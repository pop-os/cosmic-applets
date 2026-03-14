// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod localize;
mod model;
mod mouse_area;

use crate::localize::localize;
use config::{AudioAppletConfig, amplification_sink, amplification_source};
use cosmic::{
    Apply, Element, Renderer, Task, Theme, app,
    applet::{
        column as applet_column,
        cosmic_panel_config::PanelAnchor,
        menu_button, menu_control_padding, padded_control, row as applet_row,
        token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    },
    cctk::sctk::reexports::calloop,
    cosmic_config::CosmicConfigEntry,
    cosmic_theme::Spacing,
    iced::{
        self, Alignment, Length, Subscription,
        futures::StreamExt,
        widget::{self, column, row, slider},
        window,
    },
    iced_futures, surface, theme,
    widget::{Row, button, container, divider, horizontal_space, icon, text},
};
use cosmic_settings_audio_client::{self as audio_client, CosmicAudioProxy};
use cosmic_time::{Instant, Timeline, anim, chain, id};
use futures::executor::block_on;
use futures::sink::SinkExt;
use iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};

use mpris_subscription::{MprisRequest, MprisUpdate};
use mpris2_zbus::player::PlaybackStatus;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, LazyLock},
};

mod config;
mod mpris_subscription;

static SHOW_MEDIA_CONTROLS: LazyLock<id::Toggler> = LazyLock::new(id::Toggler::unique);

const GO_BACK: &str = "media-skip-backward-symbolic";
const GO_NEXT: &str = "media-skip-forward-symbolic";
const PAUSE: &str = "media-playback-pause-symbolic";
const PLAY: &str = "media-playback-start-symbolic";

pub fn run() -> cosmic::iced::Result {
    localize();
    cosmic::applet::run::<Audio>(())
}

#[derive(Default)]
pub struct Audio {
    /// For interfacing with libcosmic.
    core: cosmic::app::Core,
    /// Track the applet's popup window.
    popup: Option<window::Id>,
    /// Varlink connection to `com.system76.CosmicSettings.Audio`.
    audio_client: Option<Rc<RefCell<audio_client::Client>>>,
    /// Known audio device state
    model: model::Model,
    /// Whether to expand the revealer of a source or sink device.
    is_open: IsOpen,
    /// Max slider volume for the sink device, as determined by the amplification property.
    max_sink_volume: u32,
    /// Max slider volume for the source device, as determined by the amplification property.
    max_source_volume: u32,
    /// Breakpoints for the sink volume slider.
    sink_breakpoints: &'static [u32],
    /// Breakpoitns for the source volume slider.
    source_breakpoints: &'static [u32],
    /// Track animations used by the revealers.
    timeline: Timeline,
    /// Config file specific to this applet.
    config: AudioAppletConfig,
    /// mpris player status
    player_status: Option<mpris_subscription::PlayerStatus>,
    /// Used to request an activation token for opening cosmic-settings.
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
}

impl Audio {
    fn output_icon_name(&self) -> &'static str {
        let volume = self.model.active_sink.volume;
        let mute = self.model.active_sink.mute;
        if mute || volume == 0 {
            "audio-volume-muted-symbolic"
        } else if volume < 33 {
            "audio-volume-low-symbolic"
        } else if volume < 66 {
            "audio-volume-medium-symbolic"
        } else if volume <= 100 {
            "audio-volume-high-symbolic"
        } else {
            "audio-volume-overamplified-symbolic"
        }
    }

    fn input_icon_name(&self) -> &'static str {
        let volume = self.model.active_source.volume;
        let mute = self.model.active_source.mute;
        if mute || volume == 0 {
            "microphone-sensitivity-muted-symbolic"
        } else if volume < 33 {
            "microphone-sensitivity-low-symbolic"
        } else if volume < 66 {
            "microphone-sensitivity-medium-symbolic"
        } else {
            "microphone-sensitivity-high-symbolic"
        }
    }
}

#[derive(Debug, PartialEq, Eq, Default)]
enum IsOpen {
    #[default]
    None,
    Output,
    Input,
}

#[derive(Clone, Debug)]
pub enum Message {
    /// Connection to `com.system76.CosmicSettings`.
    Client(Arc<audio_client::Client>),
    Ignore,
    SetSinkVolume(u32),
    SetSourceVolume(u32),
    ToggleSinkMute,
    ToggleSourceMute,
    SetDefaultSink(usize),
    SetDefaultSource(usize),
    OutputToggle,
    InputToggle,
    TogglePopup,
    CloseRequested(window::Id),
    ToggleMediaControlsInTopPanel(chain::Toggler, bool),
    Frame(Instant),
    ConfigChanged(AudioAppletConfig),
    Mpris(mpris_subscription::MprisUpdate),
    MprisRequest(MprisRequest),
    Token(TokenUpdate),
    OpenSettings,
    Subscription(audio_client::Event),
    Surface(surface::Action),
}

// TODO
// mouse area with on enter and a stack widget for all buttons
// most recently entered button is on top
// position is a multiple of button size
// on leave of applet, popup button is on top again

impl Audio {
    fn playback_buttons(&self) -> Vec<Element<'_, Message>> {
        let mut elements: Vec<Element<'_, Message>> = Vec::new();
        if self.player_status.is_some() && self.config.show_media_controls_in_top_panel {
            if self
                .player_status
                .as_ref()
                .is_some_and(|s| s.can_go_previous)
            {
                elements.push(
                    self.core
                        .applet
                        .icon_button(GO_BACK)
                        .on_press(Message::MprisRequest(MprisRequest::Previous))
                        .into(),
                );
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
            if self.player_status.as_ref().is_some_and(|s| s.can_go_next) {
                elements.push(
                    self.core
                        .applet
                        .icon_button(GO_NEXT)
                        .on_press(Message::MprisRequest(MprisRequest::Next))
                        .into(),
                )
            }
        }
        elements
    }

    fn go_previous(&self, icon_size: u16) -> Option<Element<'_, Message>> {
        self.player_status.as_ref().and_then(|s| {
            if s.can_go_previous {
                Some(
                    button::icon(icon::from_name(GO_BACK).size(icon_size).symbolic(true))
                        .extra_small()
                        .class(cosmic::theme::Button::AppletIcon)
                        .on_press(Message::MprisRequest(MprisRequest::Previous))
                        .into(),
                )
            } else {
                None
            }
        })
    }

    fn go_next(&self, icon_size: u16) -> Option<Element<'_, Message>> {
        self.player_status.as_ref().and_then(|s| {
            if s.can_go_next {
                Some(
                    button::icon(icon::from_name(GO_NEXT).size(icon_size).symbolic(true))
                        .extra_small()
                        .class(cosmic::theme::Button::AppletIcon)
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
}

impl cosmic::Application for Audio {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletAudio";

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        (
            Self {
                core,
                ..Default::default()
            },
            Task::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::Frame(now) => self.timeline.now(now),
            Message::Ignore => {}
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);
                    self.timeline = Timeline::new();

                    (self.max_sink_volume, self.sink_breakpoints) = if amplification_sink() {
                        (150, &[100][..])
                    } else {
                        (100, &[][..])
                    };

                    (self.max_source_volume, self.source_breakpoints) = if amplification_source() {
                        (150, &[100][..])
                    } else {
                        (100, &[][..])
                    };

                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    return get_popup(popup_settings);
                }
            }

            Message::OutputToggle => {
                self.is_open = if self.is_open == IsOpen::Output {
                    IsOpen::None
                } else {
                    IsOpen::Output
                }
            }
            Message::InputToggle => {
                self.is_open = if self.is_open == IsOpen::Input {
                    IsOpen::None
                } else {
                    IsOpen::Input
                }
            }
            Message::Subscription(message) => {
                self.model.update(message);
            }

            Message::SetDefaultSink(pos) => {
                if let Some(&node_id) = self.model.sinks.id.get(pos) {
                    if let Some(client) = self.audio_client.as_mut() {
                        block_on(async {
                            _ = client.borrow_mut().conn.set_default(node_id).await;
                        });
                    }
                }
            }

            Message::SetDefaultSource(pos) => {
                if let Some(&node_id) = self.model.sources.id.get(pos) {
                    if let Some(client) = self.audio_client.as_mut() {
                        block_on(async {
                            _ = client.borrow_mut().conn.set_default(node_id).await;
                        });
                    }
                }
            }

            Message::ToggleSinkMute => {
                if let Some(ref mut client) = self.audio_client {
                    futures::executor::block_on(async {
                        _ = client.borrow_mut().conn.sink_mute_toggle().await;
                    });
                }
            }

            Message::ToggleSourceMute => {
                if let Some(ref mut client) = self.audio_client {
                    futures::executor::block_on(async {
                        _ = client.borrow_mut().conn.source_mute_toggle().await;
                    });
                }
            }

            Message::SetSinkVolume(volume) => {
                if let Some(ref mut client) = self.audio_client {
                    self.model.active_sink.volume = volume;
                    self.model.active_sink.volume_text = volume.to_string();
                    futures::executor::block_on(async {
                        _ = client.borrow_mut().conn.set_sink_volume(volume).await;
                    });
                }
            }

            Message::SetSourceVolume(volume) => {
                if let Some(ref mut client) = self.audio_client {
                    self.model.active_source.volume = volume;
                    self.model.active_source.volume_text = volume.to_string();
                    futures::executor::block_on(async {
                        _ = client.borrow_mut().conn.set_source_volume(volume).await;
                    });
                }
            }

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
                    return Task::none();
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
                    MprisRequest::Raise => tokio::spawn(async move {
                        let res = player.media_player().await;
                        if let Err(err) = res {
                            tracing::error!("Error fetching MediaPlayer: {}", err);
                        } else {
                            let res = res.unwrap().raise().await;
                            if let Err(err) = res {
                                tracing::error!("Error raising client: {}", err);
                            }
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
                    cmd.arg("sound");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
            },
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Message::Client(client) => {
                if let Some(client) = Arc::into_inner(client) {
                    self.audio_client = Some(Rc::new(RefCell::new(client)));
                    self.model = model::Model::default();
                }
            }
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let subscriptions = vec![
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
            Subscription::run(|| {
                iced_futures::stream::channel(1, move |mut emitter| async move {
                    loop {
                        let mut client = match audio_client::connect().await {
                            Ok(client) => client,
                            Err(why) => {
                                if let zlink::Error::Io(ref why) = why
                                    && why.kind() == std::io::ErrorKind::NotFound
                                {
                                    tracing::error!(
                                        "cosmic-settings-daemon varlink service not found. Restarting cosmic-settings-daemon"
                                    );
                                    _ = std::process::Command::new("killall")
                                        .args(&["-2", "cosmic-settings-daemon"])
                                        .status();
                                } else {
                                    tracing::error!(
                                        ?why,
                                        "failed to connect to cosmic-settings's varlink service"
                                    );
                                }

                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                continue;
                            }
                        };

                        if let Ok(Ok(mut stream)) = client.recv_events().await {
                            _ = emitter.send(Message::Client(Arc::new(client))).await;
                            while let Some(message) = stream.next().await {
                                match message {
                                    Ok(event) => {
                                        _ = emitter.send(Message::Subscription(event)).await;
                                    }
                                    Err(why) => {
                                        tracing::error!(?why, "event error");
                                    }
                                }
                            }
                        }
                    }
                })
            }),
        ];

        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        let btn = self
            .core
            .applet
            .icon_button(self.output_icon_name())
            .on_press_down(Message::TogglePopup);

        const WHEEL_STEP: f32 = 5.0; // 5% per wheel event
        let btn = crate::mouse_area::MouseArea::new(btn).on_mouse_wheel(|delta| {
            let scroll_vector = match delta {
                iced::mouse::ScrollDelta::Lines { y, .. } => y.signum() * WHEEL_STEP, // -1/0/1
                iced::mouse::ScrollDelta::Pixels { y, .. } => y.signum(),             // -1/0/1
            };
            if scroll_vector == 0.0 {
                return Message::Ignore;
            }

            let new_volume = (self.model.active_sink.volume as f64 + (scroll_vector as f64))
                .clamp(0.0, self.max_sink_volume as f64);
            Message::SetSinkVolume(new_volume as u32)
        });

        let playback_buttons = (!self.core.applet.suggested_bounds.as_ref().is_some_and(|c| {
            // if we have a configure for width and height, we're in a overflow popup
            c.width > 0. && c.height > 0.
        }))
        .then(|| self.playback_buttons());

        self.core
            .applet
            .autosize_window(
                if let Some(playback_buttons) = playback_buttons
                    && !playback_buttons.is_empty()
                {
                    match self.core.applet.anchor {
                        PanelAnchor::Left | PanelAnchor::Right => Element::from(
                            applet_column::Column::with_children(playback_buttons)
                                .push(btn)
                                .align_x(Alignment::Center)
                                // TODO configurable variable from the panel?
                                .spacing(
                                    -(self.core.applet.suggested_padding(true).0 as f32)
                                        * self.core.applet.padding_overlap,
                                ),
                        ),
                        PanelAnchor::Top | PanelAnchor::Bottom => {
                            applet_row::Row::with_children(playback_buttons)
                                .push(btn)
                                .align_y(Alignment::Center)
                                // TODO configurable variable from the panel?
                                .spacing(
                                    -(self.core.applet.suggested_padding(true).0 as f32)
                                        * self.core.applet.padding_overlap,
                                )
                                .into()
                        }
                    }
                } else {
                    btn.into()
                },
            )
            .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<'_, Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let sink = self
            .model
            .sinks
            .active
            .map(|pos| self.model.sinks.display[pos].as_str());
        let source = self
            .model
            .sources
            .active
            .map(|pos| self.model.sources.display[pos].as_str());

        let mut audio_content = {
            let output_slider = slider(
                0..=self.max_sink_volume,
                self.model.active_sink.volume,
                Message::SetSinkVolume,
            )
            .width(Length::FillPortion(5))
            .breakpoints(self.sink_breakpoints);

            let input_slider = slider(
                0..=self.max_source_volume,
                self.model.active_source.volume,
                Message::SetSourceVolume,
            )
            .width(Length::FillPortion(5))
            .breakpoints(self.source_breakpoints);

            column![
                padded_control(
                    row![
                        button::icon(
                            icon::from_name(self.output_icon_name())
                                .size(24)
                                .symbolic(true),
                        )
                        .class(cosmic::theme::Button::Icon)
                        .icon_size(24)
                        .line_height(24)
                        .on_press(Message::ToggleSinkMute),
                        output_slider,
                        container(text(&self.model.active_sink.volume_text).size(16))
                            .width(Length::FillPortion(1))
                            .align_x(Alignment::End)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center)
                ),
                padded_control(
                    row![
                        button::icon(
                            icon::from_name(self.input_icon_name())
                                .size(24)
                                .symbolic(true),
                        )
                        .class(cosmic::theme::Button::Icon)
                        .icon_size(24)
                        .line_height(24)
                        .on_press(Message::ToggleSourceMute),
                        input_slider,
                        container(text(&self.model.active_source.volume_text).size(16))
                            .width(Length::FillPortion(1))
                            .align_x(Alignment::End)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center)
                ),
                padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
                revealer(
                    self.is_open == IsOpen::Output,
                    fl!("output"),
                    match sink {
                        Some(sink) => sink.to_owned(),
                        None => fl!("no-device"),
                    },
                    &self.model.sinks.display,
                    Message::OutputToggle,
                    Message::SetDefaultSink,
                ),
                revealer(
                    self.is_open == IsOpen::Input,
                    fl!("input"),
                    match source {
                        Some(source) => source.to_owned(),
                        None => fl!("no-device"),
                    },
                    &self.model.sources.display,
                    Message::InputToggle,
                    Message::SetDefaultSource,
                )
            ]
            .align_x(Alignment::Start)
        };

        if let Some(s) = self.player_status.as_ref() {
            let mut elements = Vec::with_capacity(5);

            if let Some(icon_path) = s.icon.clone() {
                elements.push(icon(icon::from_path(icon_path)).size(36).into());
            }

            let title = if let Some(title) = s.title.as_ref() {
                if title.chars().count() > 22 {
                    let mut title_trunc = title.chars().take(20).collect::<String>();
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
                if artists.chars().count() > 27 {
                    let mut artists_trunc = artists.chars().take(25).collect::<String>();
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
            control_elements.push(horizontal_space().width(Length::Fill).into());
            if let Some(go_prev) = self.go_previous(32) {
                control_elements.push(go_prev);
            }
            if let Some(play) = self.is_play() {
                control_elements.push(
                    button::icon(icon::from_name(if play { PLAY } else { PAUSE }).symbolic(true))
                        .extra_small()
                        .class(cosmic::theme::Button::AppletIcon)
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
                    .align_y(Alignment::Center)
                    .width(Length::FillPortion(control_cnt.saturating_add(1)))
                    .spacing(8)
                    .into(),
            );

            audio_content = audio_content
                .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));
            audio_content = audio_content.push(
                menu_button(
                    Row::with_children(elements)
                        .align_y(Alignment::Center)
                        .spacing(8),
                )
                .on_press(Message::MprisRequest(MprisRequest::Raise))
                .padding(menu_control_padding()),
            );
        }
        let content = column![
            audio_content,
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
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
            ),
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
            menu_button(text::body(fl!("sound-settings"))).on_press(Message::OpenSettings)
        ]
        .align_x(Alignment::Start)
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
    devices: &[String],
    toggle: Message,
    mut change: impl FnMut(usize) -> Message + 'static,
) -> widget::Column<'static, Message, crate::Theme, Renderer> {
    if open {
        devices.iter().cloned().enumerate().fold(
            column![revealer_head(open, title, selected, toggle)].width(Length::Fill),
            |col, (id, name)| {
                col.push(
                    text::body(name)
                        .apply(menu_button)
                        .on_press(change(id))
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
    cosmic::widget::column::with_capacity(2)
        .push(text::body(title).width(Length::Fill))
        .push(text::caption(selected))
        .apply(menu_button)
        .on_press(toggle)
}
