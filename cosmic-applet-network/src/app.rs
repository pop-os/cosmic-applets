use cosmic::iced_style;
use cosmic::iced_widget::Row;
use cosmic::{
    iced::{
        wayland::popup::{destroy_popup, get_popup},
        widget::{column, container, row, scrollable, text, text_input, Column},
        Alignment, Application, Color, Command, Length, Subscription,
    },
    iced_runtime::core::{
        alignment::{Horizontal, Vertical},
        layout::Limits,
        window,
    },
    iced_style::{application, button::StyleSheet},
    theme::{Button, Svg},
    widget::{button, divider, icon, toggler},
    Element, Theme,
};
use cosmic_applet::CosmicAppletHelper;
use cosmic_dbus_networkmanager::interface::enums::{ActiveConnectionState, DeviceState};
use futures::channel::mpsc::UnboundedSender;
use zbus::Connection;

use crate::network_manager::active_conns::active_conns_subscription;
use crate::network_manager::devices::devices_subscription;
use crate::network_manager::wireless_enabled::wireless_enabled_subscription;
use crate::network_manager::NetworkManagerState;
use crate::{
    config, fl,
    network_manager::{
        available_wifi::AccessPoint, current_networks::ActiveConnectionInfo,
        network_manager_subscription, NetworkManagerEvent, NetworkManagerRequest,
    },
};

pub fn run() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    let settings = helper.window_settings();
    CosmicNetworkApplet::run(settings)
}

#[derive(Debug)]
enum NewConnectionState {
    EnterPassword {
        access_point: AccessPoint,
        password: String,
    },
    Waiting(AccessPoint),
    Failure(AccessPoint),
}

impl NewConnectionState {
    pub fn ssid(&self) -> &str {
        &match self {
            NewConnectionState::EnterPassword {
                access_point,
                password: _,
            } => access_point,
            NewConnectionState::Waiting(ap) => ap,
            NewConnectionState::Failure(ap) => ap,
        }
        .ssid
    }
}

impl Into<AccessPoint> for NewConnectionState {
    fn into(self) -> AccessPoint {
        match self {
            NewConnectionState::EnterPassword {
                access_point,
                password: _,
            } => access_point,
            NewConnectionState::Waiting(access_point) => access_point,
            NewConnectionState::Failure(access_point) => access_point,
        }
    }
}

#[derive(Default)]
struct CosmicNetworkApplet {
    icon_name: String,
    theme: Theme,
    popup: Option<window::Id>,
    id_ctr: u128,
    applet_helper: CosmicAppletHelper,
    nm_state: NetworkManagerState,
    // UI state
    nm_sender: Option<UnboundedSender<NetworkManagerRequest>>,
    show_visible_networks: bool,
    new_connection: Option<NewConnectionState>,
    conn: Option<Connection>,
}

impl CosmicNetworkApplet {
    fn update_icon_name(&mut self) {
        self.icon_name = self
            .nm_state
            .active_conns
            .iter()
            .fold("network-offline-symbolic", |icon_name, conn| {
                match (icon_name, conn) {
                    ("network-offline-symbolic", ActiveConnectionInfo::WiFi { .. }) => {
                        "network-wireless-symbolic"
                    }
                    ("network-offline-symbolic", ActiveConnectionInfo::Wired { .. })
                    | ("network-wireless-symbolic", ActiveConnectionInfo::Wired { .. }) => {
                        "network-wired-symbolic"
                    }
                    (_, ActiveConnectionInfo::Vpn { .. }) => "network-vpn-symbolic",
                    _ => icon_name,
                }
            })
            .to_string()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    ActivateKnownWifi(String),
    Disconnect(String),
    TogglePopup,
    ToggleAirplaneMode(bool),
    ToggleWiFi(bool),
    ToggleVisibleNetworks,
    Errored(String),
    Ignore,
    NetworkManagerEvent(NetworkManagerEvent),
    SelectWirelessAccessPoint(AccessPoint),
    CancelNewConnection,
    Password(String),
    SubmitPassword,
}

impl Application for CosmicNetworkApplet {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            CosmicNetworkApplet {
                icon_name: "network-offline-symbolic".to_string(),
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
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    self.show_visible_networks = false;
                    return destroy_popup(p);
                } else {
                    // TODO request update of state maybe
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
                        .min_height(1.0)
                        .min_width(1.0)
                        .max_height(800.0)
                        .max_width(400.0);
                    return get_popup(popup_settings);
                }
            }
            Message::Errored(_) => todo!(),
            Message::Ignore => {}
            Message::ToggleAirplaneMode(enabled) => {
                self.nm_state.airplane_mode = enabled;
                if let Some(tx) = self.nm_sender.as_mut() {
                    let _ = tx.unbounded_send(NetworkManagerRequest::SetAirplaneMode(enabled));
                }
            }
            Message::ToggleWiFi(enabled) => {
                if !enabled {
                    self.nm_state.clear();
                }
                self.nm_state.wifi_enabled = enabled;

                if let Some(tx) = self.nm_sender.as_mut() {
                    let _ = tx.unbounded_send(NetworkManagerRequest::SetWiFi(enabled));
                }
            }
            Message::NetworkManagerEvent(event) => match event {
                NetworkManagerEvent::Init {
                    conn,
                    sender,
                    state,
                } => {
                    self.nm_sender.replace(sender);
                    self.nm_state = state;
                    self.update_icon_name();
                    self.conn = Some(conn);
                }
                NetworkManagerEvent::WiFiEnabled(state) => {
                    self.nm_state = state;
                }
                NetworkManagerEvent::WirelessAccessPoints(state) => {
                    self.nm_state = state;
                }
                NetworkManagerEvent::ActiveConns(state) => {
                    self.nm_state = state;
                    self.update_icon_name();
                }
                NetworkManagerEvent::RequestResponse {
                    state,
                    success,
                    req,
                } => {
                    if success {
                        match req {
                            NetworkManagerRequest::SelectAccessPoint(ssid)
                            | NetworkManagerRequest::Password(ssid, _) => {
                                if self
                                    .new_connection
                                    .as_ref()
                                    .map(|c| c.ssid() == ssid)
                                    .unwrap_or_default()
                                {
                                    self.new_connection.take();
                                }
                            }
                            _ => {}
                        }
                    } else {
                        match req {
                            NetworkManagerRequest::Password(_, _) => {
                                if let Some(NewConnectionState::EnterPassword {
                                    access_point,
                                    ..
                                }) = self.new_connection.as_ref()
                                {
                                    self.new_connection
                                        .replace(NewConnectionState::Failure(access_point.clone()));
                                }
                            }
                            _ => {}
                        }
                    }
                    self.nm_state = state;
                    self.update_icon_name();
                }
            },
            Message::SelectWirelessAccessPoint(access_point) => {
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    tx
                } else {
                    return Command::none();
                };

                let _ = tx.unbounded_send(NetworkManagerRequest::SelectAccessPoint(
                    access_point.ssid.clone(),
                ));

                self.new_connection
                    .replace(NewConnectionState::EnterPassword {
                        access_point,
                        password: String::new(),
                    });
            }
            Message::ToggleVisibleNetworks => {
                self.new_connection.take();
                self.show_visible_networks = !self.show_visible_networks;
            }
            Message::Password(entered_pw) => match &mut self.new_connection {
                Some(NewConnectionState::EnterPassword { password, .. }) => {
                    *password = entered_pw;
                }
                _ => {}
            },
            Message::SubmitPassword => {
                // save password
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    tx
                } else {
                    return Command::none();
                };

                match self.new_connection.take() {
                    Some(NewConnectionState::EnterPassword {
                        password,
                        access_point,
                    }) => {
                        let _ = tx.unbounded_send(NetworkManagerRequest::Password(
                            access_point.ssid.clone(),
                            password.to_string(),
                        ));
                        self.new_connection
                            .replace(NewConnectionState::Waiting(access_point.clone()));
                    }
                    _ => {}
                };
            }
            Message::ActivateKnownWifi(ssid) => {
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    self.nm_state
                        .known_access_points
                        .iter_mut()
                        .find(|c| c.ssid == ssid)
                        .map(|ap| {
                            ap.working = true;
                        });
                    tx
                } else {
                    return Command::none();
                };
                let _ = tx.unbounded_send(NetworkManagerRequest::SelectAccessPoint(ssid));
            }
            Message::CancelNewConnection => {
                self.new_connection.take();
            }
            Message::Disconnect(ssid) => {
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    self.nm_state
                        .active_conns
                        .iter_mut()
                        .find(|c| c.name() == ssid)
                        .map(|ap| match ap {
                            ActiveConnectionInfo::WiFi { state, .. } => {
                                *state = ActiveConnectionState::Deactivating;
                            }
                            _ => {}
                        });
                    tx
                } else {
                    return Command::none();
                };
                let _ = tx.unbounded_send(NetworkManagerRequest::Disconnect(ssid));
            }
        }
        Command::none()
    }
    fn view(&self, id: window::Id) -> Element<Message> {
        let button_style = || Button::Custom {
            active: Box::new(|t| iced_style::button::Appearance {
                border_radius: 0.0,
                ..t.active(&Button::Text)
            }),
            hover: Box::new(|t| iced_style::button::Appearance {
                border_radius: 0.0,
                ..t.hovered(&Button::Text)
            }),
        };
        if id == window::Id(0) {
            self.applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into()
        } else {
            let mut vpn_ethernet_col = column![];
            let mut known_wifi = column![];
            for conn in &self.nm_state.active_conns {
                match conn {
                    ActiveConnectionInfo::Vpn { name, ip_addresses } => {
                        let mut ipv4 = Vec::with_capacity(ip_addresses.len());
                        for addr in ip_addresses {
                            ipv4.push(
                                text(format!("{}: {}", fl!("ipv4"), addr.to_string()))
                                    .size(10)
                                    .into(),
                            );
                        }
                        vpn_ethernet_col = vpn_ethernet_col
                            .push(column![text(name), Column::with_children(ipv4)].spacing(4));
                    }
                    ActiveConnectionInfo::Wired {
                        name,
                        hw_address: _,
                        speed,
                        ip_addresses,
                    } => {
                        let mut ipv4 = Vec::with_capacity(ip_addresses.len());
                        for addr in ip_addresses {
                            ipv4.push(
                                text(format!("{}: {}", fl!("ipv4"), addr.to_string()))
                                    .size(12)
                                    .into(),
                            );
                        }
                        vpn_ethernet_col = vpn_ethernet_col.push(
                            column![
                                row![
                                    text(name),
                                    text(format!("{speed} {}", fl!("megabits-per-second")))
                                ]
                                .spacing(16),
                                Column::with_children(ipv4),
                            ]
                            .spacing(4),
                        );
                    }
                    ActiveConnectionInfo::WiFi {
                        name,
                        ip_addresses,
                        state,
                        ..
                    } => {
                        let mut ipv4 = Vec::with_capacity(ip_addresses.len());
                        for addr in ip_addresses {
                            ipv4.push(
                                text(format!("{}: {}", fl!("ipv4"), addr.to_string()))
                                    .size(12)
                                    .into(),
                            );
                        }
                        let mut btn_content = vec![
                            icon("network-wireless-symbolic", 24)
                                .style(Svg::Symbolic)
                                .width(Length::Fixed(24.0))
                                .height(Length::Fixed(24.0))
                                .into(),
                            column![text(name).size(14), Column::with_children(ipv4)]
                                .width(Length::Fill)
                                .into(),
                        ];
                        match state {
                            ActiveConnectionState::Activating
                            | ActiveConnectionState::Deactivating => {
                                btn_content.push(
                                    icon("process-working-symbolic", 24)
                                        .style(Svg::Symbolic)
                                        .width(Length::Fixed(24.0))
                                        .height(Length::Fixed(24.0))
                                        .into(),
                                );
                            }
                            ActiveConnectionState::Activated => btn_content.push(
                                text(format!("{}", fl!("connected")))
                                    .size(14)
                                    .horizontal_alignment(Horizontal::Right)
                                    .vertical_alignment(Vertical::Center)
                                    .into(),
                            ),
                            _ => {}
                        };
                        known_wifi = known_wifi.push(
                            column![button(Button::Secondary)
                                .custom(vec![Row::with_children(btn_content)
                                    .align_items(Alignment::Center)
                                    .spacing(8)
                                    .into()])
                                .padding([8, 24])
                                .style(button_style())
                                .on_press(Message::Disconnect(name.clone()))]
                            .align_items(Alignment::Center),
                        );
                    }
                };
            }
            for known in &self.nm_state.known_access_points {
                let mut btn_content = vec![
                    icon("network-wireless-symbolic", 24)
                        .style(Svg::Symbolic)
                        .width(Length::Fixed(24.0))
                        .height(Length::Fixed(24.0))
                        .into(),
                    text(&known.ssid).size(14).width(Length::Fill).into(),
                ];

                if known.working {
                    btn_content.push(
                        icon("process-working-symbolic", 24)
                            .style(Svg::Symbolic)
                            .width(Length::Fixed(24.0))
                            .height(Length::Fixed(24.0))
                            .into(),
                    );
                }

                let mut btn = button(Button::Secondary)
                    .custom(vec![Row::with_children(btn_content)
                        .align_items(Alignment::Center)
                        .spacing(8)
                        .into()])
                    .padding([8, 24])
                    .width(Length::Fill)
                    .style(button_style());
                btn = match known.state {
                    DeviceState::Failed
                    | DeviceState::Unknown
                    | DeviceState::Unmanaged
                    | DeviceState::Disconnected
                    | DeviceState::NeedAuth => {
                        btn.on_press(Message::ActivateKnownWifi(known.ssid.clone()))
                    }
                    DeviceState::Activated => btn.on_press(Message::Disconnect(known.ssid.clone())),
                    _ => btn,
                };
                known_wifi = known_wifi.push(row![btn].align_items(Alignment::Center));
            }

            let mut content = column![
                vpn_ethernet_col,
                container(
                    toggler(fl!("airplane-mode"), self.nm_state.airplane_mode, |m| {
                        Message::ToggleAirplaneMode(m)
                    })
                    .text_size(14)
                    .width(Length::Fill)
                )
                .padding([0, 12]),
                divider::horizontal::light(),
                container(
                    toggler(fl!("wifi"), self.nm_state.wifi_enabled, |m| {
                        Message::ToggleWiFi(m)
                    })
                    .text_size(14)
                    .width(Length::Fill)
                )
                .padding([0, 12]),
                divider::horizontal::light(),
                known_wifi,
            ]
            .align_items(Alignment::Center)
            .spacing(8)
            .padding([8, 0]);
            let dropdown_icon = if self.show_visible_networks {
                "go-down-symbolic"
            } else {
                "go-next-symbolic"
            };
            let available_connections_btn = button(Button::Secondary)
                .custom(
                    vec![
                        text(fl!("visible-wireless-networks"))
                            .size(14)
                            .width(Length::Fill)
                            .height(Length::Fixed(24.0))
                            .vertical_alignment(Vertical::Center)
                            .into(),
                        container(
                            icon(dropdown_icon, 14)
                                .style(Svg::Symbolic)
                                .width(Length::Fixed(14.0))
                                .height(Length::Fixed(14.0)),
                        )
                        .align_x(Horizontal::Center)
                        .align_y(Vertical::Center)
                        .width(Length::Fixed(24.0))
                        .height(Length::Fixed(24.0))
                        .into(),
                    ]
                    .into(),
                )
                .padding([8, 24])
                .style(button_style())
                .on_press(Message::ToggleVisibleNetworks);
            content = content.push(available_connections_btn);
            if self.show_visible_networks {
                if let Some(new_conn_state) = self.new_connection.as_ref() {
                    match new_conn_state {
                        NewConnectionState::EnterPassword {
                            access_point,
                            password,
                        } => {
                            let id = row![
                                icon("network-wireless-symbolic", 24)
                                    .style(Svg::Symbolic)
                                    .width(Length::Fixed(24.0))
                                    .height(Length::Fixed(24.0)),
                                text(&access_point.ssid).size(14),
                            ]
                            .align_items(Alignment::Center)
                            .width(Length::Fill)
                            .padding([0, 24])
                            .spacing(12);
                            content = content.push(id);
                            let col = column![
                                text(fl!("enter-password")),
                                text_input("", password)
                                    .on_input(Message::Password)
                                    .on_paste(Message::Password)
                                    .on_submit(Message::SubmitPassword)
                                    .password(),
                                container(text(fl!("router-wps-button"))).padding(8),
                                row![
                                    button(Button::Secondary)
                                        .custom(vec![container(text(fl!("cancel")))
                                            .padding([0, 24])
                                            .into()])
                                        .on_press(Message::CancelNewConnection),
                                    button(Button::Secondary)
                                        .custom(vec![container(text(fl!("connect")))
                                            .padding([0, 24])
                                            .into()])
                                        .on_press(Message::SubmitPassword)
                                ]
                                .spacing(24)
                            ]
                            .spacing(8)
                            .padding([0, 48])
                            .align_items(Alignment::Center);
                            content = content.push(col);
                        }
                        NewConnectionState::Waiting(access_point) => {
                            let id = row![
                                icon("network-wireless-symbolic", 24)
                                    .style(Svg::Symbolic)
                                    .width(Length::Fixed(24.0))
                                    .height(Length::Fixed(24.0)),
                                text(&access_point.ssid).size(14),
                            ]
                            .align_items(Alignment::Center)
                            .width(Length::Fill)
                            .spacing(12);
                            let connecting = row![
                                id,
                                icon("process-working-symbolic", 24)
                                    .style(Svg::Symbolic)
                                    .width(Length::Fixed(24.0))
                                    .height(Length::Fixed(24.0)),
                            ]
                            .spacing(8)
                            .padding([0, 24]);
                            content = content.push(connecting);
                        }
                        NewConnectionState::Failure(access_point) => {
                            let id = row![
                                icon("network-wireless-symbolic", 24)
                                    .style(Svg::Symbolic)
                                    .width(Length::Fixed(24.0))
                                    .height(Length::Fixed(24.0)),
                                text(&access_point.ssid).size(14),
                            ]
                            .align_items(Alignment::Center)
                            .width(Length::Fill)
                            .padding([0, 24])
                            .spacing(12);
                            content = content.push(id);
                            let col = column![
                                text(fl!("unable-to-connect")),
                                text(fl!("check-wifi-connection")),
                                row![
                                    button(Button::Secondary)
                                        .custom(vec![container(text("Cancel"))
                                            .padding([0, 24])
                                            .into()])
                                        .on_press(Message::CancelNewConnection),
                                    button(Button::Secondary)
                                        .custom(vec![container(text("Connect"))
                                            .padding([0, 24])
                                            .into()])
                                        .on_press(Message::SelectWirelessAccessPoint(
                                            access_point.clone()
                                        ))
                                ]
                                .spacing(24)
                            ]
                            .spacing(16)
                            .padding([0, 48])
                            .align_items(Alignment::Center);
                            content = content.push(col);
                        }
                    }
                } else if self.nm_state.wifi_enabled {
                    let mut list_col =
                        Vec::with_capacity(self.nm_state.wireless_access_points.len());
                    for ap in &self.nm_state.wireless_access_points {
                        if self
                            .nm_state
                            .active_conns
                            .iter()
                            .any(|a| ap.ssid == a.name())
                        {
                            continue;
                        }
                        let button = button(button_style())
                            .custom(vec![row![
                                icon("network-wireless-symbolic", 16)
                                    .style(Svg::Symbolic)
                                    .width(Length::Fixed(16.0))
                                    .height(Length::Fixed(16.0)),
                                text(&ap.ssid)
                                    .size(14)
                                    .height(Length::Fixed(24.0))
                                    .vertical_alignment(Vertical::Center)
                            ]
                            .align_items(Alignment::Center)
                            .spacing(12)
                            .into()])
                            .on_press(Message::SelectWirelessAccessPoint(ap.clone()))
                            .width(Length::Fill)
                            .padding([8, 24]);
                        list_col.push(button.into());
                    }
                    content = content.push(
                        scrollable(Column::with_children(list_col)).height(Length::Fixed(300.0)),
                    );
                }
            }
            self.applet_helper.popup_container(content).into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let network_sub =
            network_manager_subscription(0).map(|e| Message::NetworkManagerEvent(e.1));

        if let Some(conn) = self.conn.as_ref() {
            Subscription::batch(vec![
                network_sub,
                active_conns_subscription(0, conn.clone())
                    .map(|e| Message::NetworkManagerEvent(e.1)),
                devices_subscription(0, conn.clone()).map(|e| Message::NetworkManagerEvent(e.1)),
                wireless_enabled_subscription(0, conn.clone())
                    .map(|e| Message::NetworkManagerEvent(e.1)),
            ])
        } else {
            network_sub
        }
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn close_requested(&self, _id: window::Id) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(Box::new(|theme| {
            application::Appearance {
                background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                text_color: theme.cosmic().on_bg_color().into(),
            }
        }))
    }
}
