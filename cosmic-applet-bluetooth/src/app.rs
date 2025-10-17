// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::bluetooth::{BluerDeviceStatus, BluerRequest, BluerState, set_discovery, set_tick};
use cosmic::{
    app,
    applet::token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    cctk::sctk::reexports::calloop,
    surface,
};

use cosmic::{
    Element, Task,
    applet::{menu_button, padded_control},
    cosmic_theme::Spacing,
    iced::{
        self, Alignment, Length, Subscription,
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        widget::{Column, column, container, row},
    },
    iced_runtime::core::window,
    theme,
    widget::{button, divider, icon, scrollable, text},
};
use cosmic_time::{Instant, Timeline, anim, chain, id};
use futures::FutureExt;
use std::{collections::HashMap, sync::LazyLock, time::Duration};
use tokio::sync::mpsc::Sender;

use crate::{
    bluetooth::{BluerDevice, BluerEvent, bluetooth_subscription},
    config, fl,
};

static BLUETOOTH_ENABLED: LazyLock<id::Toggler> = LazyLock::new(id::Toggler::unique);

#[inline]
pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicBluetoothApplet>(())
}

#[derive(Default)]
struct CosmicBluetoothApplet {
    core: cosmic::app::Core,
    icon_name: String,
    popup: Option<window::Id>,
    bluer_state: BluerState,
    bluer_sender: Option<Sender<BluerRequest>>,
    // UI state
    show_visible_devices: bool,
    request_confirmation: Option<(BluerDevice, String, Sender<bool>)>,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    timeline: Timeline,
}

impl CosmicBluetoothApplet {
    #[inline]
    fn update_icon(&mut self) {
        self.icon_name = if self.bluer_state.bluetooth_enabled {
            "cosmic-applet-bluetooth-active-symbolic"
        } else {
            "cosmic-applet-bluetooth-disabled-symbolic"
        }
        .to_string();
    }
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    ToggleVisibleDevices(bool),
    Ignore,
    BluetoothEvent(BluerEvent),
    Request(BluerRequest),
    Cancel,
    Confirm,
    Token(TokenUpdate),
    OpenSettings,
    Frame(Instant),
    ToggleBluetooth(chain::Toggler, bool),
    Surface(surface::Action),
}

impl cosmic::Application for CosmicBluetoothApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        (
            Self {
                core,
                icon_name: "bluetooth-symbolic".to_string(),
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
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    set_discovery(false);

                    return Task::batch([
                        destroy_popup(p),
                        cosmic::task::future(
                            set_tick(Duration::from_secs(10))
                                .map(|()| cosmic::Action::App(Message::Ignore)),
                        ),
                    ]);
                } else {
                    set_discovery(true);

                    // TODO request update of state maybe
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);
                    self.timeline = Timeline::new();

                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    return Task::batch([
                        get_popup(popup_settings),
                        cosmic::task::future(set_tick(Duration::from_secs(3)))
                            .map(|()| cosmic::Action::App(Message::Ignore)),
                    ]);
                }
            }
            Message::Ignore => {}
            Message::ToggleVisibleDevices(enabled) => {
                self.show_visible_devices = enabled;
            }
            Message::BluetoothEvent(e) => match e {
                BluerEvent::RequestResponse {
                    req,
                    state,
                    err_msg,
                } => {
                    if let Some(err_msg) = err_msg {
                        eprintln!("bluetooth request error: {err_msg}");
                    }
                    if self.bluer_state.bluetooth_enabled != state.bluetooth_enabled {
                        self.timeline
                            .set_chain(if state.bluetooth_enabled {
                                chain::Toggler::on(BLUETOOTH_ENABLED.clone(), 1.0)
                            } else {
                                chain::Toggler::off(BLUETOOTH_ENABLED.clone(), 1.0)
                            })
                            .start();
                    }

                    self.bluer_state = state;
                }
                BluerEvent::Init { sender, state } => {
                    self.bluer_sender.replace(sender);
                    self.bluer_state = state;
                }
                BluerEvent::DevicesChanged { state } => {
                    self.bluer_state = state;
                }
                BluerEvent::Finished => {
                    // TODO should this exit with an error causing a restart?
                    eprintln!("bluetooth subscription finished. exiting...");
                    std::process::exit(0);
                }
                // TODO handle agent events
                BluerEvent::AgentEvent(event) => match event {
                    crate::bluetooth::BluerAgentEvent::DisplayPinCode(_d, _code) => {}
                    crate::bluetooth::BluerAgentEvent::DisplayPasskey(_d, _code) => {}
                    crate::bluetooth::BluerAgentEvent::RequestPinCode(_d) => {
                        // TODO anything to be done here?
                    }
                    crate::bluetooth::BluerAgentEvent::RequestPasskey(_d) => {
                        // TODO anything to be done here?
                    }
                    crate::bluetooth::BluerAgentEvent::RequestConfirmation(d, code, tx) => {
                        self.request_confirmation.replace((d, code, tx));
                    }
                    crate::bluetooth::BluerAgentEvent::RequestDeviceAuthorization(_d, _tx) => {
                        // TODO anything to be done here?
                    }
                    crate::bluetooth::BluerAgentEvent::RequestServiceAuthorization(
                        _d,
                        _service,
                        _tx,
                    ) => {
                        // my headphones seem to always request this
                        // doesn't seem to be defined in the UX mockups
                        // dbg!(
                        //     "request service authorization",
                        //     d.name,
                        //     bluer::id::Service::try_from(service)
                        //         .map(|s| s.to_string())
                        //         .unwrap_or_else(|_| "unknown".to_string())
                        // );
                    }
                },
            },
            Message::Request(r) => {
                match &r {
                    BluerRequest::SetBluetoothEnabled(enabled) => {
                        self.bluer_state.bluetooth_enabled = *enabled;
                    }
                    BluerRequest::ConnectDevice(add) => {
                        if let Some(d) = self
                            .bluer_state
                            .devices
                            .iter_mut()
                            .find(|d| d.address == *add)
                        {
                            d.status = BluerDeviceStatus::Connecting;
                        }
                    }
                    BluerRequest::DisconnectDevice(add) => {
                        if let Some(d) = self
                            .bluer_state
                            .devices
                            .iter_mut()
                            .find(|d| d.address == *add)
                        {
                            d.status = BluerDeviceStatus::Disconnecting;
                        }
                    }
                    BluerRequest::PairDevice(add) => {
                        if let Some(d) = self
                            .bluer_state
                            .devices
                            .iter_mut()
                            .find(|d| d.address == *add)
                        {
                            d.status = BluerDeviceStatus::Pairing;
                        }
                    }
                    _ => {} // TODO
                }
                if let Some(tx) = self.bluer_sender.clone() {
                    tokio::spawn(async move {
                        let _ = tx.send(r).await;
                    });
                }
            }
            Message::Cancel => {
                if let Some((_, _, tx)) = self.request_confirmation.take() {
                    tokio::spawn(async move {
                        let _ = tx.send(false).await;
                    });
                }
            }
            Message::Confirm => {
                if let Some((_, _, tx)) = self.request_confirmation.take() {
                    tokio::spawn(async move {
                        let _ = tx.send(true).await;
                    });
                }
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
                return cosmic::task::future(
                    set_tick(Duration::from_secs(10))
                        .map(|()| cosmic::Action::App(Message::Ignore)),
                );
            }
            Message::OpenSettings => {
                let exec = "cosmic-settings bluetooth".to_string();
                if let Some(tx) = self.token_tx.as_ref() {
                    let _ = tx.send(TokenRequest {
                        app_id: Self::APP_ID.to_string(),
                        exec,
                    });
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
                    cmd.arg("bluetooth");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
            },
            Message::Frame(instant) => self.timeline.now(instant),
            Message::ToggleBluetooth(chain, enabled) => {
                if self.bluer_state.bluetooth_enabled == enabled {
                    return Task::none();
                }
                self.timeline.set_chain(chain).start();
                self.bluer_state.bluetooth_enabled = enabled;
                if let Some(tx) = self.bluer_sender.clone() {
                    tokio::spawn(async move {
                        let _ = tx.send(BluerRequest::SetBluetoothEnabled(enabled)).await;
                    });
                }
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }
        self.update_icon();
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button(&self.icon_name)
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<'_, Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let mut known_bluetooth = Vec::new();
        // PERF: This should be pre-filtered in an update.
        for dev in self.bluer_state.devices.iter().filter(|d| {
            self.request_confirmation
                .as_ref()
                .is_none_or(|(dev, _, _)| d.address != dev.address)
        }) {
            let mut row = row![
                icon::from_name(dev.icon).size(16).symbolic(true),
                text::body(dev.name.as_str())
                    .align_x(Alignment::Start)
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
            ]
            .align_y(Alignment::Center)
            .spacing(12);

            if let Some(battery) = dev.battery_percent {
                let icon = match battery {
                    b if (20..40).contains(&b) => "battery-low",
                    b if b < 20 => "battery-caution",
                    _ => "battery",
                };
                let status = row!(
                    icon::from_name(icon).symbolic(true).size(14),
                    text::body(format!("{battery}%"))
                )
                .align_y(Alignment::Center)
                .spacing(2)
                .width(Length::Shrink);

                let content = container(status)
                    .align_x(Alignment::End)
                    .align_y(Alignment::Center);

                row = row.push(content);
            }

            match &dev.status {
                BluerDeviceStatus::Connected => {
                    row = row.push(
                        text::body(fl!("connected"))
                            .align_x(Alignment::End)
                            .align_y(Alignment::Center),
                    );
                }
                BluerDeviceStatus::Paired => {}
                BluerDeviceStatus::Connecting | BluerDeviceStatus::Disconnecting => {
                    row = row.push(
                        icon::from_name("process-working-symbolic")
                            .size(24)
                            .symbolic(true),
                    );
                }
                BluerDeviceStatus::Disconnected | BluerDeviceStatus::Pairing => continue,
            }

            known_bluetooth.push(
                menu_button(row)
                    .on_press(match dev.status {
                        BluerDeviceStatus::Connected => {
                            Message::Request(BluerRequest::DisconnectDevice(dev.address))
                        }
                        BluerDeviceStatus::Disconnected => {
                            Message::Request(BluerRequest::PairDevice(dev.address))
                        }
                        BluerDeviceStatus::Paired => {
                            Message::Request(BluerRequest::ConnectDevice(dev.address))
                        }
                        BluerDeviceStatus::Connecting => {
                            Message::Request(BluerRequest::CancelConnect(dev.address))
                        }
                        BluerDeviceStatus::Disconnecting => Message::Ignore, // Start connecting?
                        BluerDeviceStatus::Pairing => Message::Ignore,       // Cancel pairing?
                    })
                    .into(),
            );
        }

        let mut content = column![column![padded_control(
            anim!(
                //toggler
                BLUETOOTH_ENABLED,
                &self.timeline,
                fl!("bluetooth"),
                self.bluer_state.bluetooth_enabled,
                Message::ToggleBluetooth,
            )
            .text_size(14)
            .width(Length::Fill)
        ),],]
        .align_x(Alignment::Center)
        .padding([8, 0]);
        if !known_bluetooth.is_empty() {
            content = content
                .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));
            content = content.push(Column::with_children(known_bluetooth));
        }
        let dropdown_icon = if self.show_visible_devices {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };
        let available_connections_btn = menu_button(row![
            text::body(fl!("other-devices"))
                .width(Length::Fill)
                .height(Length::Fixed(24.0))
                .align_y(Alignment::Center),
            container(icon::from_name(dropdown_icon).size(16).symbolic(true))
                .center(Length::Fixed(24.0))
        ])
        .on_press(Message::ToggleVisibleDevices(!self.show_visible_devices));
        if self.bluer_state.bluetooth_enabled {
            content = content
                .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));
            content = content.push(available_connections_btn);
        }
        let mut list_column: Vec<Element<'_, Message>> =
            Vec::with_capacity(self.bluer_state.devices.len());

        if let Some((device, pin, _)) = self.request_confirmation.as_ref() {
            let row = column![
                padded_control(row![
                    icon::from_name(device.icon).size(16).symbolic(true),
                    text::body(&device.name)
                        .align_x(Alignment::Start)
                        .align_y(Alignment::Center)
                        .width(Length::Fill)
                ]),
                padded_control(
                    text::body(fl!(
                        "confirm-pin",
                        HashMap::from([("deviceName", device.name.clone())])
                    ))
                    .align_x(Alignment::Start)
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                ),
                padded_control(text::title3(pin).center().width(Length::Fixed(280.0)))
                    .align_x(Alignment::Center),
                padded_control(
                    row![
                        button::custom(text::body(fl!("cancel")).center())
                            .padding([4, 0])
                            .height(Length::Fixed(28.0))
                            .width(Length::Fixed(105.0))
                            .on_press(Message::Cancel),
                        button::custom(text::body(fl!("confirm")).center())
                            .padding([4, 0])
                            .height(Length::Fixed(28.0))
                            .width(Length::Fixed(105.0))
                            .on_press(Message::Confirm),
                    ]
                    .spacing(self.core.system_theme().cosmic().space_xxs())
                    .width(Length::Shrink)
                    .align_y(Alignment::Center)
                )
                .align_x(Alignment::Center)
            ];
            list_column.push(row.into());
        }
        let mut visible_devices_count = 0;
        if self.show_visible_devices && self.bluer_state.bluetooth_enabled {
            let mut visible_devices = column![];
            for dev in self.bluer_state.devices.iter().filter(|d| {
                matches!(
                    d.status,
                    BluerDeviceStatus::Disconnected | BluerDeviceStatus::Pairing
                ) && self
                    .request_confirmation
                    .as_ref()
                    .is_none_or(|(dev, _, _)| d.address != dev.address)
                    && (d.has_name() || d.is_known_device_type())
            }) {
                let row = row![
                    icon::from_name(dev.icon).size(16).symbolic(true),
                    text::body(dev.name.clone()).align_x(Alignment::Start),
                ]
                .align_y(Alignment::Center)
                .spacing(12);
                visible_devices = visible_devices.push(
                    menu_button(row.width(Length::Fill))
                        .on_press(Message::Request(BluerRequest::PairDevice(dev.address))),
                );
                visible_devices_count += 1;
            }
            list_column.push(visible_devices.into());
        }
        let item_counter = visible_devices_count
                // request confirmation is pretty big
                + if self.request_confirmation.is_some() {
                    5
                } else {
                    0
                };

        if item_counter > 10 {
            content = content
                .push(scrollable(Column::with_children(list_column)).height(Length::Fixed(300.0)));
        } else {
            content = content.push(Column::with_children(list_column));
        }
        content = content
            .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]))
            .push(menu_button(text::body(fl!("settings"))).on_press(Message::OpenSettings));

        self.core.applet.popup_container(content).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            activation_token_subscription(0).map(Message::Token),
            bluetooth_subscription(0).map(Message::BluetoothEvent),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
        ])
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}
