use cosmic::{
    applet::CosmicAppletHelper,
    iced::{
        executor,
        widget::{column, container, row, scrollable, text},
        Alignment, Application, Color, Command, Length, Subscription,
    },
    iced_native::window,
    iced_style::{application, svg},
    theme::{Button, Svg},
    widget::{button, horizontal_rule, icon, list_column, toggler},
    Element, Theme,
};
use futures::channel::mpsc::UnboundedSender;
use iced_sctk::{
    application::SurfaceIdWrapper,
    commands::popup::{destroy_popup, get_popup},
};

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

#[derive(Clone, Default)]
struct CosmicNetworkApplet {
    icon_name: String,
    theme: Theme,
    popup: Option<window::Id>,
    id_ctr: u32,
    applet_helper: CosmicAppletHelper,
    // STATE
    airplane_mode: bool,
    wifi: bool,
    wireless_access_points: Vec<AccessPoint>,
    active_conns: Vec<ActiveConnectionInfo>,
    nm_sender: Option<UnboundedSender<NetworkManagerRequest>>,
}

impl CosmicNetworkApplet {
    fn update_icon_name(&mut self) {
        self.icon_name = self
        .active_conns
        .iter()
        .fold("network-offline-symbolic", |icon_name, conn| {
            match (icon_name, conn) {
                ("network-offline-symbolic", ActiveConnectionInfo::WiFi { .. }) => {
                    "network-wireless-symbolic"
                }
                (
                    "network-offline-symbolic",
                    ActiveConnectionInfo::Wired { .. },
                )
                | (
                    "network-wireless-symbolic",
                    ActiveConnectionInfo::Wired { .. },
                ) => "network-wired-symbolic",
                (_, ActiveConnectionInfo::Vpn { .. }) => "network-vpn-symbolic",
                _ => icon_name,
            }
        })
        .to_string()
    }
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    ToggleAirplaneMode(bool),
    ToggleWiFi(bool),
    Errored(String),
    Ignore,
    NetworkManagerEvent(NetworkManagerEvent),
    SelectWirelessAccessPoint(String),
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

                    let popup_settings = self.applet_helper.get_popup_settings(
                        window::Id::new(0),
                        new_id,
                        (420, 600),
                        None,
                        None,
                    );
                    return get_popup(popup_settings);
                }
            }
            Message::Errored(_) => todo!(),
            Message::Ignore => {}
            Message::ToggleAirplaneMode(enabled) => {
                self.airplane_mode = enabled;
                // TODO apply changes
            }
            Message::ToggleWiFi(enabled) => {
                self.wifi = enabled;
                if let Some(tx) = self.nm_sender.as_mut() {
                    let _ = tx.unbounded_send(NetworkManagerRequest::SetWiFi(enabled));
                }
            }
            Message::NetworkManagerEvent(event) => match event {
                NetworkManagerEvent::Init {
                    sender,
                    wireless_access_points,
                    active_conns,
                    wifi_enabled,
                    airplane_mode,
                } => {
                    self.nm_sender.replace(sender);
                    self.wireless_access_points = wireless_access_points;
                    self.active_conns = active_conns;
                    self.wifi = wifi_enabled;
                    self.airplane_mode = airplane_mode;
                    self.update_icon_name();
                }
                NetworkManagerEvent::WiFiEnabled(enabled) => {
                    self.wifi = enabled;
                }
                NetworkManagerEvent::WirelessAccessPoints(access_points) => {
                    self.wireless_access_points = access_points;
                }
                NetworkManagerEvent::ActiveConns(conns) => {
                    self.active_conns = conns;
                    self.update_icon_name();
                }
                NetworkManagerEvent::RequestResponse { wireless_access_points, active_conns, wifi_enabled, success, ..} => {
                    if success {
                        self.wireless_access_points = wireless_access_points;
                        self.active_conns = active_conns;
                        self.wifi = wifi_enabled;
                        self.update_icon_name();
                    }
                },
            },
            Message::SelectWirelessAccessPoint(ssid) => {
                if let Some(tx) = self.nm_sender.as_ref() {
                    let _ = tx.unbounded_send(NetworkManagerRequest::SelectAccessPoint(ssid));
                }
            }
        }
        Command::none()
    }
    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => self
                .applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into(),
            SurfaceIdWrapper::Popup(_) => {
                let name = text(fl!("network")).size(18);
                let icon = icon(&self.icon_name, 24)
                    .style(Svg::Custom(|theme| svg::Appearance {
                        fill: Some(theme.palette().text),
                    }))
                    .width(Length::Units(24))
                    .height(Length::Units(24));
                let mut list_col = list_column();

                for conn in &self.active_conns {
                    let el = match conn {
                        ActiveConnectionInfo::Vpn { name, ip_addresses } => {
                            let mut ipv4 = column![];
                            let mut ipv6 = column![];
                            for addr in ip_addresses {
                                match addr {
                                    std::net::IpAddr::V4(a) => {
                                        ipv4 = ipv4.push(text(format!(
                                            "{}: {}",
                                            fl!("ipv4"),
                                            a.to_string()
                                        )));
                                    }
                                    std::net::IpAddr::V6(a) => {
                                        ipv6 = ipv6.push(text(format!(
                                            "{}: {}",
                                            fl!("ipv6"),
                                            a.to_string()
                                        )));
                                    }
                                }
                            }
                            column![text(name), ipv4, ipv6].spacing(4)
                        }
                        ActiveConnectionInfo::Wired {
                            name,
                            hw_address,
                            speed,
                            ip_addresses,
                        } => {
                            let mut ipv4 = column![];
                            let mut ipv6 = column![];
                            for addr in ip_addresses {
                                match addr {
                                    std::net::IpAddr::V4(a) => {
                                        ipv4 = ipv4.push(text(format!(
                                            "{}: {}",
                                            fl!("ipv4"),
                                            a.to_string()
                                        )));
                                    }
                                    std::net::IpAddr::V6(a) => {
                                        ipv6 = ipv6.push(text(format!(
                                            "{}: {}",
                                            fl!("ipv6"),
                                            a.to_string()
                                        )));
                                    }
                                }
                            }
                            column![
                                row![
                                    text(name),
                                    text(format!("{speed} {}", fl!("megabits-per-second")))
                                ]
                                .spacing(16),
                                ipv4,
                                ipv6,
                                text(format!("{}: {hw_address}", fl!("mac"))),
                            ]
                            .spacing(4)
                        }
                        ActiveConnectionInfo::WiFi {
                            name, hw_address, ..
                        } => column![row![
                            text(name),
                            text(format!("{}: {hw_address}", fl!("mac")))
                        ]
                        .spacing(12)]
                        .spacing(4),
                    };
                    list_col = list_col.add(el);
                }

                let mut content = column![
                    row![icon, name].spacing(8).width(Length::Fill),
                    list_col,
                    horizontal_rule(1),
                    container(
                        toggler(fl!("airplane-mode"), self.airplane_mode, |m| {
                            Message::ToggleAirplaneMode(m)
                        })
                        .width(Length::Fill)
                    )
                    .padding([0, 12]),
                    horizontal_rule(1),
                    container(
                        toggler(fl!("wifi"), self.wifi, |m| { Message::ToggleWiFi(m) })
                            .width(Length::Fill)
                    )
                    .padding([0, 12]),
                ]
                .align_items(Alignment::Center)
                .spacing(8)
                .padding(8);
                if self.wifi {
                    let mut list_col = list_column();
                    for ap in &self.wireless_access_points {
                        let button = self
                            .active_conns
                            .iter()
                            .find_map(|conn| match conn {
                                ActiveConnectionInfo::WiFi { name, .. } if name == &ap.ssid => {
                                    Some(
                                        button(Button::Primary)
                                            .text(&ap.ssid)
                                            .on_press(Message::Ignore)
                                            .width(Length::Fill),
                                    )
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| {
                                button(Button::Text)
                                    .text(&ap.ssid)
                                    .on_press(Message::SelectWirelessAccessPoint(ap.ssid.clone()))
                                    .width(Length::Fill)
                            });
                        list_col = list_col.add(button);
                    }
                    content = content.push(scrollable(list_col).height(Length::Fill));
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

    fn close_requested(&self, _id: iced_sctk::application::SurfaceIdWrapper) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| application::Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }
}
