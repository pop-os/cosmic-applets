// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::bluetooth::{BluerDeviceStatus, BluerRequest, BluerState, DeviceProperty};
use cosmic::{
    applet::token::subscription::{activation_token_subscription, TokenRequest, TokenUpdate},
    cctk::sctk::reexports::calloop,
    widget::text::body,
};

use cosmic::{
    applet::{menu_button, padded_control},
    iced::{
        self,
        wayland::popup::{destroy_popup, get_popup},
        widget::{column, container, row, scrollable, text, Column},
        Alignment, Length, Subscription,
    },
    iced_runtime::core::{
        alignment::{Horizontal, Vertical},
        layout::Limits,
        window,
    },
    iced_style::application,
    widget::{button, divider, icon},
    Command, Element, Theme,
};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc::Sender;

use crate::{
    bluetooth::{bluetooth_subscription, BluerDevice, BluerEvent},
    config, fl,
};

static BLUETOOTH_ENABLED: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicBluetoothApplet>(false, ())
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
}

impl cosmic::Application for CosmicBluetoothApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (Self, iced::Command<cosmic::app::Message<Self::Message>>) {
        (
            Self {
                core,
                icon_name: "bluetooth-symbolic".to_string(),
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
    ) -> iced::Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    // TODO request update of state maybe
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
                        .max_height(800.0)
                        .max_width(400.0);
                    let tx = self.bluer_sender.as_ref().cloned();
                    return Command::batch(vec![
                        iced::Command::perform(
                            async {
                                if let Some(tx) = tx {
                                    let _ = tx.send(BluerRequest::StateUpdate).await;
                                }
                            },
                            |_| cosmic::app::message::app(Message::Ignore),
                        ),
                        get_popup(popup_settings),
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
                        eprintln!("bluetooth request error: {}", err_msg);
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
                    // TODO special handling for some requests
                    match req {
                        BluerRequest::StateUpdate
                            if self.popup.is_some() && self.bluer_sender.is_some() =>
                        {
                            let tx = self.bluer_sender.as_ref().cloned().unwrap();
                            tokio::spawn(async move {
                                // sleep for a bit before requesting state update again
                                tokio::time::sleep(Duration::from_millis(3000)).await;
                                let _ = tx.send(BluerRequest::StateUpdate).await;
                            });
                        }
                        _ => {}
                    };
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
                if let Some(tx) = self.bluer_sender.as_mut().cloned() {
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
            }
            Message::OpenSettings => {
                let exec = "cosmic-settings bluetooth".to_string();
                if let Some(tx) = self.token_tx.as_ref() {
                    let _ = tx.send(TokenRequest {
                        app_id: Self::APP_ID.to_string(),
                        exec,
                    });
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
                    return Command::none();
                }
                self.timeline.set_chain(chain).start();
                self.bluer_state.bluetooth_enabled = enabled;
                if let Some(tx) = self.bluer_sender.clone() {
                    tokio::spawn(async move {
                        let _ = tx.send(BluerRequest::SetBluetoothEnabled(enabled)).await;
                    });
                }
            }
        }
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
        let mut known_bluetooth = vec![];
        for dev in self.bluer_state.devices.iter().filter(|d| {
            !self
                .request_confirmation
                .as_ref()
                .map_or(false, |(dev, _, _)| d.address == dev.address)
        }) {
            let mut row = row![
                icon::from_name(dev.icon.as_str()).size(16).symbolic(true),
                text(dev.name.clone())
                    .size(14)
                    .horizontal_alignment(Horizontal::Left)
                    .vertical_alignment(Vertical::Center)
                    .width(Length::Fill)
            ]
            .align_items(Alignment::Center)
            .spacing(12);

            if let Some(DeviceProperty::BatteryPercentage(battery)) = dev
                .properties
                .iter()
                .find(|p| matches!(p, DeviceProperty::BatteryPercentage(_)))
            {
                let icon = match *battery {
                    b if b >= 20 && b < 40 => "battery-low",
                    b if b < 20 => "battery-caution",
                    _ => "battery",
                };
                let status = row!(
                    icon::from_name(icon).symbolic(true).size(14),
                    body(format!("{}%", battery))
                )
                .align_items(Alignment::Center)
                .spacing(2)
                .width(Length::Shrink);

                let content = container(status)
                    .align_x(Horizontal::Right)
                    .align_y(Vertical::Center);

                row = row.push(content);
            }

            match &dev.status {
                BluerDeviceStatus::Connected => {
                    row = row.push(
                        text(fl!("connected"))
                            .size(14)
                            .horizontal_alignment(Horizontal::Right)
                            .vertical_alignment(Vertical::Center),
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
            };

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
        .align_items(Alignment::Center)
        .padding([8, 0]);
        if !known_bluetooth.is_empty() {
            content = content.push(padded_control(divider::horizontal::default()));
            content = content.push(Column::with_children(known_bluetooth));
        }
        let dropdown_icon = if self.show_visible_devices {
            "go-down-symbolic"
        } else {
            "go-next-symbolic"
        };
        let available_connections_btn = menu_button(row![
            text(fl!("other-devices"))
                .size(14)
                .width(Length::Fill)
                .height(Length::Fixed(24.0))
                .vertical_alignment(Vertical::Center),
            container(icon::from_name(dropdown_icon).size(14).symbolic(true))
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
        ])
        .on_press(Message::ToggleVisibleDevices(!self.show_visible_devices));
        content = content.push(padded_control(divider::horizontal::default()));
        content = content.push(available_connections_btn);
        let mut list_column: Vec<Element<'_, Message>> =
            Vec::with_capacity(self.bluer_state.devices.len());

        if let Some((device, pin, _)) = self.request_confirmation.as_ref() {
            let row = column![
                padded_control(row![
                    icon::from_name(device.icon.as_str())
                        .size(16)
                        .symbolic(true),
                    text(&device.name)
                        .size(14)
                        .horizontal_alignment(Horizontal::Left)
                        .vertical_alignment(Vertical::Center)
                        .width(Length::Fill)
                ]),
                padded_control(
                    text(fl!(
                        "confirm-pin",
                        HashMap::from_iter(vec![("deviceName", device.name.clone())])
                    ))
                    .horizontal_alignment(Horizontal::Left)
                    .vertical_alignment(Vertical::Center)
                    .width(Length::Fill)
                    .size(14)
                ),
                padded_control(
                    text(pin)
                        .horizontal_alignment(Horizontal::Center)
                        .vertical_alignment(Vertical::Center)
                        .width(Length::Fixed(280.0))
                        .size(22)
                )
                .align_x(Horizontal::Center),
                padded_control(
                    row![
                        button(
                            text(fl!("cancel"))
                                .size(14)
                                .width(Length::Fill)
                                .height(Length::Fixed(24.0))
                                .vertical_alignment(Vertical::Center)
                                .horizontal_alignment(Horizontal::Center)
                        )
                        .padding([8, 24])
                        .width(Length::Fill)
                        .on_press(Message::Cancel),
                        button(
                            text(fl!("confirm"))
                                .size(14)
                                .height(Length::Fixed(24.0))
                                .vertical_alignment(Vertical::Center)
                                .horizontal_alignment(Horizontal::Center)
                        )
                        .padding([8, 24])
                        .width(Length::Fill)
                        .on_press(Message::Confirm),
                    ]
                    .spacing(self.core.system_theme().cosmic().space_xxs())
                    .width(Length::Fill)
                    .align_items(Alignment::Center)
                )
                .align_x(Horizontal::Center)
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
                ) && !self
                    .request_confirmation
                    .as_ref()
                    .map_or(false, |(dev, _, _)| d.address == dev.address)
            }) {
                let row = row![
                    icon::from_name(dev.icon.as_str()).size(16).symbolic(true),
                    text(dev.name.clone())
                        .horizontal_alignment(Horizontal::Left)
                        .size(14),
                ]
                .align_items(Alignment::Center)
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

        self.core.applet.popup_container(content).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            activation_token_subscription(0).map(Message::Token),
            bluetooth_subscription(0).map(Message::BluetoothEvent),
            self.timeline
                .as_subscription()
                .map(|(_, now)| Message::Frame(now)),
        ])
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}
