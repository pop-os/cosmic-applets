// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    backend::{self, wayland::WaylandUpdate},
    fl,
};
use cctk::sctk::reexports::calloop;
use cosmic::{
    Element, Task, app,
    applet::{
        menu_button, padded_control,
        token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    },
    cctk::sctk::reexports::calloop::channel,
    cosmic_config::{self, CosmicConfigEntry},
    cosmic_theme::{CosmicPalette, Spacing, ThemeBuilder},
    iced::{
        Length, Subscription,
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        window,
    },
    surface,
    theme::{self, CosmicTheme},
    widget::{Column, divider, text, toggler},
};

use cosmic_settings_a11y_manager_subscription::{
    self as cosmic_a11y_manager, AccessibilityEvent, AccessibilityRequest, ColorFilter,
};
use cosmic_settings_accessibility_subscription::{self as accessibility};
use std::sync::LazyLock;
use tokio::sync::mpsc::UnboundedSender;

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicA11yApplet>(())
}

#[derive(Clone, Default)]
struct CosmicA11yApplet {
    core: cosmic::app::Core,
    high_contrast: Option<bool>,
    reader_enabled: bool,
    magnifier_enabled: bool,
    inverted_colors_enabled: bool,
    popup: Option<window::Id>,
    dbus_sender: Option<UnboundedSender<accessibility::Request>>,
    wayland_sender: Option<calloop::channel::Sender<AccessibilityRequest>>,
    wayland_protocol_version: Option<u32>,
    token_tx: Option<channel::Sender<TokenRequest>>,
    screen_filter_active: bool,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    HighContrastEnabled(bool),
    ScreenReaderEnabled(bool),
    MagnifierEnabled(bool),
    InvertedColorsEnabled(bool),
    FilterColorsEnabled(bool),
    Token(TokenUpdate),
    OpenSettings,
    DBusUpdate(accessibility::Response),
    WaylandUpdate(WaylandUpdate),
    Surface(surface::Action),
}

impl cosmic::Application for CosmicA11yApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletA11y";

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
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

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::ScreenReaderEnabled(enabled) => {
                if let Some(tx) = &self.dbus_sender {
                    self.reader_enabled = enabled;
                    let _ = tx.send(accessibility::Request::ScreenReader(enabled));
                } else {
                    self.reader_enabled = false;
                }
            }
            Message::MagnifierEnabled(enabled) => {
                if let Some(tx) = &self.wayland_sender {
                    self.magnifier_enabled = enabled;
                    let _ = tx.send(AccessibilityRequest::Magnifier(enabled));
                } else {
                    self.magnifier_enabled = false;
                }
            }
            Message::InvertedColorsEnabled(enabled) => {
                if let Some(tx) = &self.wayland_sender {
                    self.inverted_colors_enabled = enabled;
                    let _ = tx.send(AccessibilityRequest::ScreenFilter {
                        inverted: enabled,
                        filter: None,
                    });
                } else {
                    self.inverted_colors_enabled = false;
                }
            }
            Message::FilterColorsEnabled(enabled) => {
                if let Some(sender) = self.wayland_sender.as_ref() {
                    self.screen_filter_active = enabled;
                    let _ = sender.send(AccessibilityRequest::ScreenFilter {
                        inverted: self.inverted_colors_enabled,
                        filter: enabled.then_some(ColorFilter::Unknown),
                    });
                }
            }
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);

                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        Some((1, 1)),
                        None,
                        None,
                    );

                    return get_popup(popup_settings);
                }
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
            Message::HighContrastEnabled(enabled) => {
                if self.core.system_theme().cosmic().is_high_contrast == enabled
                    || self.high_contrast.is_some_and(|hc| hc == enabled)
                {
                    return Task::none();
                }

                self.high_contrast = Some(enabled);

                _ = std::thread::spawn(move || {
                    let set_hc = |is_dark: bool| {
                        let builder_config = if is_dark {
                            ThemeBuilder::dark_config()?
                        } else {
                            ThemeBuilder::light_config()?
                        };
                        let mut builder = match ThemeBuilder::get_entry(&builder_config) {
                            Ok(b) => b,
                            Err((errs, b)) => {
                                tracing::warn!("{errs:?}");
                                b
                            }
                        };

                        builder.palette = if is_dark {
                            if enabled {
                                CosmicPalette::HighContrastDark(builder.palette.inner())
                            } else {
                                CosmicPalette::Dark(builder.palette.inner())
                            }
                        } else if enabled {
                            CosmicPalette::HighContrastLight(builder.palette.inner())
                        } else {
                            CosmicPalette::Light(builder.palette.inner())
                        };
                        builder.write_entry(&builder_config)?;

                        let new_theme = builder.build();

                        let theme_config = if is_dark {
                            CosmicTheme::dark_config()?
                        } else {
                            CosmicTheme::light_config()?
                        };

                        new_theme.write_entry(&theme_config)?;

                        Result::<(), cosmic_config::Error>::Ok(())
                    };
                    if let Err(err) = set_hc(true) {
                        tracing::warn!("{err:?}");
                    }
                    if let Err(err) = set_hc(false) {
                        tracing::warn!("{err:?}");
                    }
                });
            }
            Message::OpenSettings => {
                let exec = "cosmic-settings accessibility".to_string();
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
                    cmd.arg("accessibility");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
            },
            Message::DBusUpdate(update) => match update {
                accessibility::Response::Error(err) => {
                    tracing::error!("{err}");
                    let _ = self.dbus_sender.take();
                    self.reader_enabled = false;
                }
                accessibility::Response::ScreenReader(enabled) => {
                    self.reader_enabled = enabled;
                }
                accessibility::Response::Init(enabled, tx) => {
                    self.reader_enabled = enabled;
                    self.dbus_sender = Some(tx);
                }
                _ => (),
            },
            Message::WaylandUpdate(update) => match update {
                WaylandUpdate::Errored => {
                    tracing::error!("Wayland error");
                    let _ = self.wayland_sender.take();
                    self.wayland_protocol_version = None;
                    self.magnifier_enabled = false;
                    self.inverted_colors_enabled = false;
                }
                WaylandUpdate::State(AccessibilityEvent::Bound(ver)) => {
                    self.wayland_protocol_version = Some(ver);
                }
                WaylandUpdate::State(AccessibilityEvent::Magnifier(enabled)) => {
                    self.magnifier_enabled = enabled;
                }
                WaylandUpdate::State(AccessibilityEvent::ScreenFilter { inverted, .. }) => {
                    self.inverted_colors_enabled = inverted;
                }
                WaylandUpdate::State(AccessibilityEvent::Closed) => {
                    self.screen_filter_active = false;
                    self.wayland_sender = None;
                    self.wayland_protocol_version = None;
                }
                WaylandUpdate::Started(tx) => {
                    self.wayland_sender = Some(tx);
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

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button("preferences-desktop-accessibility-symbolic")
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<'_, Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let reader_toggle = padded_control(
            toggler(self.reader_enabled)
                .on_toggle(Message::ScreenReaderEnabled)
                .text_size(14)
                .width(Length::Fill)
                .label(fl!("screen-reader")),
        );
        let magnifier_toggle = padded_control(
            toggler(self.magnifier_enabled)
                .on_toggle(Message::MagnifierEnabled)
                .text_size(14)
                .width(Length::Fill)
                .label(fl!("magnifier")),
        );
        let invert_colors_toggle = padded_control(
            toggler(self.inverted_colors_enabled)
                .on_toggle(Message::InvertedColorsEnabled)
                .text_size(14)
                .width(Length::Fill)
                .label(fl!("invert-colors")),
        );

        let hc_colors_toggle = padded_control(
            toggler(
                self.high_contrast
                    .unwrap_or(self.core.system_theme().cosmic().is_high_contrast),
            )
            .on_toggle(Message::HighContrastEnabled)
            .label(fl!("high-contrast"))
            .text_size(14)
            .width(Length::Fill),
        );

        let filter_colors_toggle = padded_control(
            toggler(self.screen_filter_active)
                .on_toggle(Message::FilterColorsEnabled)
                .label(fl!("filter-colors"))
                .width(Length::Fill)
                .text_size(14),
        );

        let content_list = Column::with_capacity(5)
            .push(reader_toggle)
            .push_maybe(
                self.wayland_protocol_version
                    .is_some()
                    .then_some(magnifier_toggle),
            )
            .push_maybe(
                self.wayland_protocol_version
                    .is_some_and(|ver| ver >= 2)
                    .then_some(invert_colors_toggle),
            )
            .push_maybe(
                self.wayland_protocol_version
                    .is_some_and(|ver| ver >= 3)
                    .then_some(filter_colors_toggle),
            )
            .push(hc_colors_toggle)
            .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]))
            .push(menu_button(text::body(fl!("settings"))).on_press(Message::OpenSettings))
            .padding([8, 0]);

        self.core.applet.popup_container(content_list).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            accessibility::subscription().map(Message::DBusUpdate),
            backend::wayland::a11y_subscription().map(Message::WaylandUpdate),
            activation_token_subscription(0).map(Message::Token),
        ])
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}
