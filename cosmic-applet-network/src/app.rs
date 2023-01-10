use cosmic::iced_style;
use cosmic::{
    applet::CosmicAppletHelper,
    iced::{
        executor,
        wayland::{
            popup::{destroy_popup, get_popup},
            SurfaceIdWrapper,
        },
        widget::{column, container, row, scrollable, text, text_input},
        Alignment, Application, Color, Command, Length, Subscription,
    },
    iced_native::{
        alignment::{Horizontal, Vertical},
        layout::Limits,
        renderer::BorderRadius,
        subscription, window,
    },
    iced_style::{application, button::StyleSheet, svg},
    theme::{Button, Svg},
    widget::{button, horizontal_rule, icon, list_column, toggler},
    Element, Theme,
};
use cosmic_dbus_networkmanager::{access_point, interface::enums::DeviceState};
use futures::channel::mpsc::UnboundedSender;

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
    CosmicNetworkApplet::run(helper.window_settings())
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

impl Into<AccessPoint> for NewConnectionState {
    fn into(self) -> AccessPoint {
        match self {
            NewConnectionState::EnterPassword {
                access_point,
                password,
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
    id_ctr: u32,
    applet_helper: CosmicAppletHelper,
    nm_state: NetworkManagerState,
    // UI state
    nm_sender: Option<UnboundedSender<NetworkManagerRequest>>,
    show_visible_networks: bool,
    new_connection: Option<NewConnectionState>,
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
enum Message {
    ActivateKnownWifi(String),
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
    type Executor = executor::Default;
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
                    return destroy_popup(p);
                } else {
                    // TODO request update of state maybe
                    self.id_ctr += 1;
                    let new_id = window::Id::new(self.id_ctr);
                    self.popup.replace(new_id);

                    let mut popup_settings = self.applet_helper.get_popup_settings(
                        window::Id::new(0),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .min_height(1)
                        .min_width(1)
                        .max_height(600)
                        .max_width(600);
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
                NetworkManagerEvent::Init { sender, state } => {
                    self.nm_sender.replace(sender);
                    self.nm_state = state;
                    self.update_icon_name();
                }
                NetworkManagerEvent::WiFiEnabled(enabled) => {
                    if !enabled {
                        self.nm_state.clear();
                    }
                    self.nm_state.wifi_enabled = enabled;
                }
                NetworkManagerEvent::WirelessAccessPoints(access_points) => {
                    self.nm_state.wireless_access_points = access_points;
                }
                NetworkManagerEvent::ActiveConns(conns) => {
                    self.nm_state.active_conns = conns;
                    self.update_icon_name();
                }
                NetworkManagerEvent::RequestResponse {
                    state,
                    success,
                    req,
                } => {
                    if success {
                        match req {
                            NetworkManagerRequest::SetAirplaneMode(_)
                            | NetworkManagerRequest::SetWiFi(_) => {}
                            NetworkManagerRequest::SelectAccessPoint(_)
                            | NetworkManagerRequest::Password(_, _) => {
                                dbg!("success");
                                dbg!(&state);
                                self.new_connection.take();
                                self.show_visible_networks = false;
                            }
                        }
                    } else {
                        match req {
                            NetworkManagerRequest::SetAirplaneMode(_)
                            | NetworkManagerRequest::SetWiFi(_) => {}
                            NetworkManagerRequest::SelectAccessPoint(_) => {
                                if let Some(NewConnectionState::Waiting(access_point)) =
                                    self.new_connection.as_ref()
                                {
                                    self.new_connection
                                        .replace(NewConnectionState::Failure(access_point.clone()));
                                }
                            }
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
                    tx
                } else {
                    return Command::none();
                };
                let _ = tx.unbounded_send(NetworkManagerRequest::SelectAccessPoint(ssid));
            }
            Message::CancelNewConnection => {
                self.new_connection.take();
            }
        }
        Command::none()
    }
    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        let button_style = Button::Custom {
            active: |t| iced_style::button::Appearance {
                border_radius: BorderRadius::from(0.0),
                ..t.active(&Button::Text)
            },
            hover: |t| iced_style::button::Appearance {
                border_radius: BorderRadius::from(0.0),
                ..t.hovered(&Button::Text)
            },
        };
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => self
                .applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into(),
            SurfaceIdWrapper::Popup(_) => {
                let mut vpn_ethernet_col = column![];
                let mut known_wifi = column![];
                for conn in &self.nm_state.active_conns {
                    match conn {
                        ActiveConnectionInfo::Vpn { name, ip_addresses } => {
                            let mut ipv4 = column![];
                            for addr in ip_addresses {
                                match addr {
                                    std::net::IpAddr::V4(a) => {
                                        ipv4 = ipv4.push(
                                            text(format!("{}: {}", fl!("ipv4"), a.to_string()))
                                                .size(12),
                                        );
                                    }
                                    std::net::IpAddr::V6(_) => {}
                                }
                            }
                            vpn_ethernet_col =
                                vpn_ethernet_col.push(column![text(name), ipv4].spacing(4));
                        }
                        ActiveConnectionInfo::Wired {
                            name,
                            hw_address,
                            speed,
                            ip_addresses,
                        } => {
                            let mut ipv4 = column![];
                            for addr in ip_addresses {
                                match addr {
                                    std::net::IpAddr::V4(a) => {
                                        ipv4 = ipv4.push(
                                            text(format!("{}: {}", fl!("ipv4"), a.to_string()))
                                                .size(12),
                                        );
                                    }
                                    std::net::IpAddr::V6(a) => {}
                                }
                            }
                            vpn_ethernet_col = vpn_ethernet_col.push(
                                column![
                                    row![
                                        text(name),
                                        text(format!("{speed} {}", fl!("megabits-per-second")))
                                    ]
                                    .spacing(16),
                                    ipv4,
                                ]
                                .spacing(4),
                            );
                        }
                        ActiveConnectionInfo::WiFi {
                            name, ip_addresses, ..
                        } => {
                            let mut ipv4 = column![];
                            for addr in ip_addresses {
                                match addr {
                                    std::net::IpAddr::V4(a) => {
                                        ipv4 = ipv4.push(
                                            text(format!("{}: {}", fl!("ipv4"), a.to_string()))
                                                .size(12),
                                        );
                                    }
                                    std::net::IpAddr::V6(_) => {}
                                }
                            }
                            known_wifi = known_wifi.push(column![button(Button::Secondary)
                                .custom(vec![
                                    icon("network-wireless-symbolic", 24)
                                        .style(Svg::Custom(|theme| svg::Appearance {
                                            color: Some(theme.palette().text),
                                        }))
                                        .width(Length::Units(24))
                                        .height(Length::Units(24))
                                        .into(),
                                    column![text(name).size(14), ipv4,].into(),
                                    text(format!("{}", fl!("connected")))
                                        .size(14)
                                        .width(Length::Fill)
                                        .height(Length::Units(24))
                                        .horizontal_alignment(Horizontal::Right)
                                        .vertical_alignment(Vertical::Center)
                                        .into()
                                ])
                                .padding([8, 24])
                                .style(button_style.clone())]);
                        }
                    };
                }
                for known in &self.nm_state.known_access_points {
                    let mut btn = button(Button::Secondary)
                        .custom(vec![
                            icon("network-wireless-symbolic", 24)
                                .style(Svg::Custom(|theme| svg::Appearance {
                                    color: Some(theme.palette().text),
                                }))
                                .width(Length::Units(24))
                                .height(Length::Units(24))
                                .into(),
                            text(&known.ssid).size(14).into(),
                        ])
                        .padding([8, 24])
                        .width(Length::Fill)
                        .style(button_style.clone());
                    btn = match known.state {
                        // DeviceState::Prepare => todo!(),
                        // DeviceState::Config => todo!(),
                        // DeviceState::NeedAuth => todo!(),
                        // DeviceState::IpConfig => todo!(),
                        // DeviceState::IpCheck => todo!(),
                        // DeviceState::Secondaries => todo!(),
                        DeviceState::Failed
                        | DeviceState::Unknown
                        | DeviceState::Unmanaged
                        | DeviceState::Disconnected
                        | DeviceState::NeedAuth => {
                            btn.on_press(Message::ActivateKnownWifi(known.ssid.clone()))
                        }
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
                        .width(Length::Fill)
                    )
                    .padding([0, 12]),
                    horizontal_rule(1),
                    container(
                        toggler(fl!("wifi"), self.nm_state.wifi_enabled, |m| {
                            Message::ToggleWiFi(m)
                        })
                        .width(Length::Fill)
                    )
                    .padding([0, 12]),
                    horizontal_rule(1),
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
                                .height(Length::Units(24))
                                .vertical_alignment(Vertical::Center)
                                .into(),
                            container(
                                icon(dropdown_icon, 14)
                                    .style(Svg::Custom(|theme| svg::Appearance {
                                        color: Some(theme.palette().text),
                                    }))
                                    .width(Length::Units(14))
                                    .height(Length::Units(14)),
                            )
                            .align_x(Horizontal::Center)
                            .align_y(Vertical::Center)
                            .width(Length::Units(24))
                            .height(Length::Units(24))
                            .into(),
                        ]
                        .into(),
                    )
                    .padding([8, 24])
                    .style(button_style.clone())
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
                                        .style(Svg::Custom(|theme| svg::Appearance {
                                            color: Some(theme.palette().text),
                                        }))
                                        .width(Length::Units(24))
                                        .height(Length::Units(24)),
                                    text(&access_point.ssid).size(14),
                                ]
                                .align_items(Alignment::Center)
                                .width(Length::Fill)
                                .padding([0, 24])
                                .spacing(12);
                                content = content.push(id);
                                let col = column![
                                    text(fl!("enter-password")),
                                    text_input("", password, Message::Password)
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
                                        .style(Svg::Custom(|theme| svg::Appearance {
                                            color: Some(theme.palette().text),
                                        }))
                                        .width(Length::Units(24))
                                        .height(Length::Units(24)),
                                    text(&access_point.ssid).size(14),
                                ]
                                .align_items(Alignment::Center)
                                .width(Length::Fill)
                                .spacing(12);
                                let connecting = row![
                                    id,
                                    icon("process-working-symbolic", 24)
                                        .style(Svg::Custom(|theme| svg::Appearance {
                                            color: Some(theme.palette().text),
                                        }))
                                        .width(Length::Units(24))
                                        .height(Length::Units(24)),
                                ]
                                .spacing(8)
                                .padding([0, 24]);
                                content = content.push(connecting);
                            }
                            NewConnectionState::Failure(access_point) => {
                                let id = row![
                                    icon("network-wireless-symbolic", 24)
                                        .style(Svg::Custom(|theme| svg::Appearance {
                                            color: Some(theme.palette().text),
                                        }))
                                        .width(Length::Units(24))
                                        .height(Length::Units(24)),
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
                        let mut list_col = column![];
                        for ap in &self.nm_state.wireless_access_points {
                            if self
                                .nm_state
                                .active_conns
                                .iter()
                                .any(|a| ap.ssid == a.name())
                            {
                                continue;
                            }
                            let button = button(button_style)
                                .custom(vec![row![
                                    icon("network-wireless-symbolic", 16)
                                        .style(Svg::Custom(|theme| svg::Appearance {
                                            color: Some(theme.palette().text),
                                        }))
                                        .width(Length::Units(16))
                                        .height(Length::Units(16)),
                                    text(&ap.ssid)
                                        .size(14)
                                        .height(Length::Units(24))
                                        .vertical_alignment(Vertical::Center)
                                ]
                                .align_items(Alignment::Center)
                                .spacing(12)
                                .into()])
                                .on_press(Message::SelectWirelessAccessPoint(ap.clone()))
                                .width(Length::Fill)
                                .padding([8, 24]);
                            list_col = list_col.push(button);
                        }
                        content = content.push(scrollable(list_col).height(Length::Units(300)));
                    }
                }
                self.applet_helper.popup_container(content).into()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        network_manager_subscription(0).map(|(_, event)| Message::NetworkManagerEvent(event))
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn close_requested(&self, _id: SurfaceIdWrapper) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| application::Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }
}
