// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    backend::{self, A11yRequest},
    fl,
};
use cosmic::{
    applet::{
        menu_button, padded_control,
        token::subscription::{activation_token_subscription, TokenRequest, TokenUpdate},
    },
    cctk::sctk::reexports::calloop,
    cosmic_theme::Spacing,
    iced::{
        alignment::Horizontal,
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        widget::{column, container, row, slider},
        window, Alignment, Length, Subscription,
    },
    iced_core::{alignment::Vertical, Background, Border, Color, Shadow},
    iced_runtime::core::layout::Limits,
    iced_widget::{Column, Row},
    theme,
    widget::{divider, horizontal_space, icon, scrollable, text, vertical_space},
    Element, Task,
};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};

use std::{collections::HashMap, path::PathBuf, time::Duration};
use tokio::sync::mpsc::UnboundedSender;

static ENABLED: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicA11yApplet>(())
}

#[derive(Clone, Default)]
struct CosmicA11yApplet {
    core: cosmic::app::Core,
    icon_name: String,
    a11y_enabled: bool,
    popup: Option<window::Id>,
    a11y_sender: Option<UnboundedSender<backend::A11yRequest>>,
    timeline: Timeline,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    Errored(String),
    Enabled(chain::Toggler, bool),
    Frame(Instant),
    Token(TokenUpdate),
    OpenSettings,
    Update(backend::Update),
}

impl cosmic::Application for CosmicA11yApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletA11y";

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (
        Self,
        cosmic::iced::Task<cosmic::app::Message<Self::Message>>,
    ) {
        (
            Self {
                core,
                token_tx: None,

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

    fn update(
        &mut self,
        message: Self::Message,
    ) -> cosmic::iced::Task<cosmic::app::Message<Self::Message>> {
        match message {
            Message::Frame(now) => self.timeline.now(now),
            Message::Enabled(chain, enabled) => {
                self.timeline.set_chain(chain).start();
                self.a11y_enabled = enabled;

                if let Some(tx) = &self.a11y_sender {
                    let _ = tx.send(A11yRequest::Status(enabled));
                }
            }
            Message::Errored(why) => {
                tracing::error!("{}", why);
            }
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    self.timeline = Timeline::new();

                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        Some((1, 1)),
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(300.0)
                        .min_width(200.0)
                        .min_height(10.0)
                        .max_height(1080.0);

                    return get_popup(popup_settings);
                }
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
            Message::OpenSettings => {
                let exec = "cosmic-settings a11y".to_string();
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
                    cmd.arg("a11y");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
            },
            Message::Update(update) => match update {
                backend::Update::Error(err) => {
                    tracing::error!("{err}");
                }
                backend::Update::Status(enabled) => {
                    self.a11y_enabled = enabled;
                }
                backend::Update::Init(enabled, tx) => {
                    self.a11y_enabled = enabled;
                    self.a11y_sender = Some(tx);
                }
            },
        }
        Task::none()
    }

    fn view(&self) -> Element<Message> {
        self.core
            .applet
            .icon_button("preferences-desktop-accessibility-symbolic")
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let toggle = padded_control(
            anim!(
                //toggler
                ENABLED,
                &self.timeline,
                fl!("accessibility"),
                self.a11y_enabled,
                Message::Enabled,
            )
            .text_size(14)
            .width(Length::Fill),
        );

        self.core
            .applet
            .popup_container(toggle.padding([8, 8]))
            .max_width(372.)
            .max_height(600.)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            backend::subscription().map(Message::Update),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
            activation_token_subscription(0).map(Message::Token),
        ])
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
