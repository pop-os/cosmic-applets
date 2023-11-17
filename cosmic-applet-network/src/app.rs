use cosmic::app::Command;
use cosmic::applet::{menu_button, menu_control_padding, padded_control};
use cosmic::iced_widget::Row;
use cosmic::{
    iced::{
        wayland::popup::{destroy_popup, get_popup},
        widget::{column, container, row, scrollable, text, text_input, Column},
        Alignment, Length, Subscription,
    },
    iced_runtime::core::{
        alignment::{Horizontal, Vertical},
        layout::Limits,
        window,
    },
    iced_style::application,
    theme::Button,
    widget::{button, divider, icon},
    Element, Theme,
};
use cosmic_dbus_networkmanager::interface::enums::{ActiveConnectionState, DeviceState};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};

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
    cosmic::applet::run::<CosmicNetworkApplet>(false, ())
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
            Self::EnterPassword {
                access_point,
                password: _,
            } => access_point,
            Self::Waiting(ap) => ap,
            Self::Failure(ap) => ap,
        }
        .ssid
    }
}

impl From<NewConnectionState> for AccessPoint {
    fn from(connection_state: NewConnectionState) -> Self {
        match connection_state {
            NewConnectionState::EnterPassword {
                access_point,
                password: _,
            } => access_point,
            NewConnectionState::Waiting(access_point) => access_point,
            NewConnectionState::Failure(access_point) => access_point,
        }
    }
}

static WIFI: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);
static AIRPLANE_MODE: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

#[derive(Default)]
struct CosmicNetworkApplet {
    core: cosmic::app::Core,
    icon_name: String,
    popup: Option<window::Id>,
    id_ctr: u128,
    nm_state: NetworkManagerState,
    // UI state
    nm_sender: Option<UnboundedSender<NetworkManagerRequest>>,
    show_visible_networks: bool,
    new_connection: Option<NewConnectionState>,
    conn: Option<Connection>,
    timeline: Timeline,
    toggle_wifi_ctr: u128,
}

fn wifi_icon(strength: u8) -> &'static str {
    if strength < 25 {
        "network-wireless-signal-weak-symbolic"
    } else if strength < 50 {
        "network-wireless-signal-ok-symbolic"
    } else if strength < 75 {
        "network-wireless-signal-good-symbolic"
    } else {
        "network-wireless-signal-excellent-symbolic"
    }
}

impl CosmicNetworkApplet {
    fn update_nm_state(&mut self, new_state: NetworkManagerState) {
        self.update_togglers(&new_state);
        self.nm_state = new_state;
        self.update_icon_name();
    }

    fn update_icon_name(&mut self) {
        self.icon_name = self
            .nm_state
            .active_conns
            .iter()
            .fold(
                "network-wired-disconnected-symbolic",
                |icon_name, conn| match (icon_name, conn) {
                    (
                        "network-wired-disconnected-symbolic",
                        ActiveConnectionInfo::WiFi { strength, .. },
                    ) => wifi_icon(*strength),
                    (_, ActiveConnectionInfo::Wired { .. })
                        if icon_name != "network-vpn-symbolic" =>
                    {
                        "network-wired-symbolic"
                    }
                    (_, ActiveConnectionInfo::Vpn { .. }) => "network-vpn-symbolic",
                    _ => icon_name,
                },
            )
            .to_string()
    }

    fn update_togglers(&mut self, state: &NetworkManagerState) {
        let timeline = &mut self.timeline;
        let mut changed = false;
        if state.wifi_enabled != self.nm_state.wifi_enabled {
            changed = true;
            let chain = if state.wifi_enabled {
                chain::Toggler::on(WIFI.clone(), 1.)
            } else {
                chain::Toggler::off(WIFI.clone(), 1.)
            };
            timeline.set_chain(chain);
        };

        if state.airplane_mode != self.nm_state.airplane_mode {
            changed = true;
            let chain = if state.airplane_mode {
                chain::Toggler::on(AIRPLANE_MODE.clone(), 1.)
            } else {
                chain::Toggler::off(AIRPLANE_MODE.clone(), 1.)
            };
            timeline.set_chain(chain);
        };
        if changed {
            timeline.start();
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    ActivateKnownWifi(String),
    Disconnect(String),
    TogglePopup,
    CloseRequested(window::Id),
    ToggleAirplaneMode(bool),
    ToggleWiFi(bool),
    ToggleVisibleNetworks,
    NetworkManagerEvent(NetworkManagerEvent),
    SelectWirelessAccessPoint(AccessPoint),
    CancelNewConnection,
    Password(String),
    SubmitPassword,
    Frame(Instant),
    // Errored(String),
}

impl cosmic::Application for CosmicNetworkApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                core,
                icon_name: "network-offline-symbolic".to_string(),
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

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Frame(now) => self.timeline.now(now),
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    self.show_visible_networks = false;
                    return destroy_popup(p);
                } else {
                    // TODO request update of state maybe
                    self.id_ctr += 1;
                    let new_id = window::Id(self.id_ctr);
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
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
                    if let Some(tx) = self.nm_sender.as_mut() {
                        let _ = tx.unbounded_send(NetworkManagerRequest::Reload);
                    }
                    return get_popup(popup_settings);
                }
            }
            // Message::Errored(_) => todo!(),
            Message::ToggleAirplaneMode(enabled) => {
                self.toggle_wifi_ctr += 1;
                if let Some(tx) = self.nm_sender.as_mut() {
                    let _ = tx.unbounded_send(NetworkManagerRequest::SetAirplaneMode(enabled));
                }
            }
            Message::ToggleWiFi(enabled) => {
                self.toggle_wifi_ctr += 1;

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
                    self.update_nm_state(state);
                    self.conn = Some(conn);
                }
                NetworkManagerEvent::WiFiEnabled(state)
                | NetworkManagerEvent::WirelessAccessPoints(state)
                | NetworkManagerEvent::ActiveConns(state) => {
                    self.update_nm_state(state);
                }
                NetworkManagerEvent::RequestResponse {
                    state,
                    success,
                    req,
                } => {
                    if let NetworkManagerRequest::SelectAccessPoint(ssid)
                    | NetworkManagerRequest::Password(ssid, _)
                    | NetworkManagerRequest::Disconnect(ssid) = &req
                    {
                        if self
                            .new_connection
                            .as_ref()
                            .map(|c| c.ssid() != ssid)
                            .unwrap_or_default()
                        {
                            self.new_connection = None;
                        }
                    }
                    if !success {
                        if let NetworkManagerRequest::Password(_, _) = req {
                            if let Some(
                                NewConnectionState::EnterPassword { access_point, .. }
                                | NewConnectionState::Waiting(access_point),
                            ) = self.new_connection.as_ref()
                            {
                                self.new_connection
                                    .replace(NewConnectionState::Failure(access_point.clone()));
                            }
                        }
                    }
                    self.update_nm_state(state);
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
            Message::Password(entered_pw) => {
                if let Some(NewConnectionState::EnterPassword { password, .. }) =
                    &mut self.new_connection
                {
                    *password = entered_pw;
                }
            }
            Message::SubmitPassword => {
                // save password
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    tx
                } else {
                    return Command::none();
                };

                if let Some(NewConnectionState::EnterPassword {
                    password,
                    access_point,
                }) = self.new_connection.take()
                {
                    let _ = tx.unbounded_send(NetworkManagerRequest::Password(
                        access_point.ssid.clone(),
                        password,
                    ));
                    self.new_connection
                        .replace(NewConnectionState::Waiting(access_point));
                };
            }
            Message::ActivateKnownWifi(ssid) => {
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    if let Some(ap) = self
                        .nm_state
                        .known_access_points
                        .iter_mut()
                        .find(|c| c.ssid == ssid)
                    {
                        ap.working = true;
                    }
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
                    if let Some(ActiveConnectionInfo::WiFi { state, .. }) = self
                        .nm_state
                        .active_conns
                        .iter_mut()
                        .find(|c| c.name() == ssid)
                    {
                        *state = ActiveConnectionState::Deactivating;
                    }
                    tx
                } else {
                    return Command::none();
                };
                let _ = tx.unbounded_send(NetworkManagerRequest::Disconnect(ssid));
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
        }
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
        let mut vpn_ethernet_col = column![];
        let mut known_wifi = column![];
        for conn in &self.nm_state.active_conns {
            match conn {
                ActiveConnectionInfo::Vpn { name, ip_addresses } => {
                    let mut ipv4 = Vec::with_capacity(ip_addresses.len() + 1);
                    ipv4.push(text(name).size(14).into());
                    for addr in ip_addresses {
                        ipv4.push(text(format!("{}: {}", fl!("ipv4"), addr)).size(10).into());
                    }
                    vpn_ethernet_col = vpn_ethernet_col.push(column![
                        row![
                            icon(
                                icon::from_name(self.icon_name.clone())
                                    .symbolic(true)
                                    .into()
                            )
                            .size(40),
                            Column::with_children(ipv4),
                            text(fl!("connected"))
                                .width(Length::Fill)
                                .horizontal_alignment(Horizontal::Right)
                                .size(14),
                        ]
                        .align_items(Alignment::Center)
                        .spacing(8)
                        .padding(menu_control_padding()),
                        padded_control(divider::horizontal::default()),
                    ]);
                }
                ActiveConnectionInfo::Wired {
                    name,
                    hw_address: _,
                    speed,
                    ip_addresses,
                } => {
                    let mut ipv4 = Vec::with_capacity(ip_addresses.len() + 1);
                    ipv4.push(text(name).size(14).into());
                    for addr in ip_addresses {
                        ipv4.push(text(format!("{}: {}", fl!("ipv4"), addr)).size(12).into());
                    }

                    vpn_ethernet_col = vpn_ethernet_col.push(column![
                        row![
                            icon(
                                icon::from_name(self.icon_name.clone())
                                    .symbolic(true)
                                    .into()
                            )
                            .size(40),
                            Column::with_children(ipv4),
                            text(format!(
                                "{} - {speed} {}",
                                fl!("connected"),
                                fl!("megabits-per-second")
                            ))
                            .width(Length::Fill)
                            .horizontal_alignment(Horizontal::Right)
                            .size(14),
                        ]
                        .align_items(Alignment::Center)
                        .spacing(8)
                        .padding(menu_control_padding()),
                        padded_control(divider::horizontal::default()),
                    ]);
                }
                ActiveConnectionInfo::WiFi {
                    name,
                    ip_addresses,
                    state,
                    strength,
                    ..
                } => {
                    let mut ipv4 = Vec::with_capacity(ip_addresses.len());
                    for addr in ip_addresses {
                        ipv4.push(text(format!("{}: {}", fl!("ipv4"), addr)).size(12).into());
                    }
                    let mut btn_content = vec![
                        icon::from_name(wifi_icon(*strength))
                            .size(24)
                            .symbolic(true)
                            .into(),
                        column![text(name).size(14), Column::with_children(ipv4)]
                            .width(Length::Fill)
                            .into(),
                    ];
                    match state {
                        ActiveConnectionState::Activating | ActiveConnectionState::Deactivating => {
                            btn_content.push(
                                icon::from_name("process-working-symbolic")
                                    .size(24)
                                    .symbolic(true)
                                    .into(),
                            );
                        }
                        ActiveConnectionState::Activated => btn_content.push(
                            text(fl!("connected").to_string())
                                .size(14)
                                .horizontal_alignment(Horizontal::Right)
                                .vertical_alignment(Vertical::Center)
                                .into(),
                        ),
                        _ => {}
                    };
                    known_wifi = known_wifi.push(
                        column![menu_button(
                            Row::with_children(btn_content)
                                .align_items(Alignment::Center)
                                .spacing(8)
                        )
                        .on_press(Message::Disconnect(name.clone()))]
                        .align_items(Alignment::Center),
                    );
                }
            };
        }

        let mut content = column![
            vpn_ethernet_col,
            padded_control(
                anim!(
                    //toggler
                    AIRPLANE_MODE,
                    &self.timeline,
                    fl!("airplane-mode"),
                    self.nm_state.airplane_mode,
                    |_chain, enable| { Message::ToggleAirplaneMode(enable) },
                )
                .text_size(14)
                .width(Length::Fill)
            ),
            padded_control(divider::horizontal::default()),
            padded_control(
                anim!(
                    //toggler
                    WIFI,
                    &self.timeline,
                    fl!("wifi"),
                    self.nm_state.wifi_enabled,
                    |_chain, enable| { Message::ToggleWiFi(enable) },
                )
                .text_size(14)
                .width(Length::Fill)
            ),
            padded_control(divider::horizontal::default()),
        ]
        .align_items(Alignment::Center);
        if self.nm_state.airplane_mode {
            content = content.push(
                column!(
                    icon::from_name("airplane-mode-symbolic")
                        .size(48)
                        .symbolic(true),
                    text(fl!("airplane-mode-on")).size(14),
                    text(fl!("turn-off-airplane-mode")).size(12)
                )
                .spacing(8)
                .align_items(Alignment::Center)
                .width(Length::Fill),
            );
        } else {
            for known in &self.nm_state.known_access_points {
                let mut btn_content = Vec::with_capacity(2);

                let ssid = text(&known.ssid).size(14).width(Length::Fill);
                if known.working {
                    btn_content.push(
                        icon::from_name("network-wireless-acquiring-symbolic")
                            .size(24)
                            .symbolic(true)
                            .into(),
                    );
                    btn_content.push(ssid.into());
                    btn_content.push(
                        icon::from_name("process-working-symbolic")
                            .size(24)
                            .symbolic(true)
                            .into(),
                    );
                } else if matches!(known.state, DeviceState::Unavailable) {
                    btn_content.push(
                        icon::from_name("network-wireless-disconnected-symbolic")
                            .size(24)
                            .symbolic(true)
                            .into(),
                    );
                    btn_content.push(ssid.into());
                } else {
                    btn_content.push(
                        icon::from_name(wifi_icon(known.strength))
                            .size(24)
                            .symbolic(true)
                            .into(),
                    );
                    btn_content.push(ssid.into());
                }

                let mut btn = menu_button(
                    Row::with_children(btn_content)
                        .align_items(Alignment::Center)
                        .spacing(8),
                );
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
            content = content.push(known_wifi);
            let dropdown_icon = if self.show_visible_networks {
                "go-down-symbolic"
            } else {
                "go-next-symbolic"
            };
            let available_connections_btn = menu_button(row![
                text(fl!("visible-wireless-networks"))
                    .size(14)
                    .width(Length::Fill)
                    .height(Length::Fixed(24.0))
                    .vertical_alignment(Vertical::Center),
                container(icon::from_name(dropdown_icon).size(14).symbolic(true))
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .width(Length::Fixed(24.0))
                    .height(Length::Fixed(24.0)),
            ])
            .on_press(Message::ToggleVisibleNetworks);
            content = content.push(padded_control(divider::horizontal::default()));
            content = content.push(available_connections_btn);
        }
        if self.show_visible_networks {
            if let Some(new_conn_state) = self.new_connection.as_ref() {
                match new_conn_state {
                    NewConnectionState::EnterPassword {
                        access_point,
                        password,
                    } => {
                        let id = padded_control(
                            row![
                                icon::from_name("network-wireless-acquiring-symbolic")
                                    .size(24)
                                    .symbolic(true),
                                text(&access_point.ssid).size(14),
                            ]
                            .align_items(Alignment::Center)
                            .spacing(12),
                        );
                        content = content.push(id);
                        let col = padded_control(
                            column![
                                text(fl!("enter-password")),
                                text_input("", password)
                                    .on_input(Message::Password)
                                    .on_paste(Message::Password)
                                    .on_submit(Message::SubmitPassword)
                                    .password(),
                                container(text(fl!("router-wps-button"))).padding(8),
                                row![
                                    button(container(text(fl!("cancel"))).padding([0, 24]))
                                        .on_press(Message::CancelNewConnection),
                                    button(container(text(fl!("connect"))).padding([0, 24]))
                                        .style(Button::Suggested)
                                        .on_press(Message::SubmitPassword)
                                ]
                                .spacing(24)
                            ]
                            .spacing(8)
                            .align_items(Alignment::Center),
                        )
                        .align_x(Horizontal::Center);
                        content = content.push(col);
                    }
                    NewConnectionState::Waiting(access_point) => {
                        let id = row![
                            icon::from_name("network-wireless-acquiring-symbolic")
                                .size(24)
                                .symbolic(true),
                            text(&access_point.ssid).size(14),
                        ]
                        .align_items(Alignment::Center)
                        .width(Length::Fill)
                        .spacing(12);
                        let connecting = padded_control(
                            row![
                                id,
                                icon::from_name("process-working-symbolic")
                                    .size(24)
                                    .symbolic(true),
                            ]
                            .spacing(8),
                        );
                        content = content.push(connecting);
                    }
                    NewConnectionState::Failure(access_point) => {
                        let id = padded_control(
                            row![
                                icon::from_name("network-wireless-error-symbolic")
                                    .size(24)
                                    .symbolic(true),
                                text(&access_point.ssid).size(14),
                            ]
                            .align_items(Alignment::Center)
                            .spacing(12),
                        )
                        .align_x(Horizontal::Center);
                        content = content.push(id);
                        let col = padded_control(
                            column![
                                text(fl!("unable-to-connect")),
                                text(fl!("check-wifi-connection")),
                                row![
                                    button(container(text("Cancel")).padding([0, 24]))
                                        .on_press(Message::CancelNewConnection),
                                    button(container(text("Connect")).padding([0, 24]))
                                        .style(Button::Suggested)
                                        .on_press(Message::SelectWirelessAccessPoint(
                                            access_point.clone()
                                        ))
                                ]
                                .spacing(24)
                            ]
                            .spacing(16)
                            .align_items(Alignment::Center),
                        )
                        .align_x(Horizontal::Center);
                        content = content.push(col);
                    }
                }
            } else if self.nm_state.wifi_enabled {
                let mut list_col = Vec::with_capacity(self.nm_state.wireless_access_points.len());
                for ap in &self.nm_state.wireless_access_points {
                    if self
                        .nm_state
                        .active_conns
                        .iter()
                        .any(|a| ap.ssid == a.name())
                    {
                        continue;
                    }
                    let button = menu_button(
                        row![
                            icon::from_name(wifi_icon(ap.strength))
                                .size(16)
                                .symbolic(true),
                            text(&ap.ssid)
                                .size(14)
                                .height(Length::Fixed(24.0))
                                .vertical_alignment(Vertical::Center)
                        ]
                        .align_items(Alignment::Center)
                        .spacing(12),
                    )
                    .on_press(Message::SelectWirelessAccessPoint(ap.clone()));
                    list_col.push(button.into());
                }
                content = content
                    .push(scrollable(Column::with_children(list_col)).height(Length::Fixed(300.0)));
            }
        }
        content = content.push(padded_control(divider::horizontal::default()));
        content = content.push(menu_button(text(fl!("settings")).size(14)));
        self.core
            .applet
            .popup_container(content.padding([8, 0, 8, 0]))
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let network_sub = network_manager_subscription(0).map(Message::NetworkManagerEvent);
        let timeline = self
            .timeline
            .as_subscription()
            .map(|(_, now)| Message::Frame(now));

        if let Some(conn) = self.conn.as_ref() {
            let has_popup = self.popup.is_some();
            Subscription::batch(vec![
                timeline,
                network_sub,
                active_conns_subscription(self.toggle_wifi_ctr, conn.clone())
                    .map(Message::NetworkManagerEvent),
                devices_subscription(self.toggle_wifi_ctr, has_popup, conn.clone())
                    .map(Message::NetworkManagerEvent),
                wireless_enabled_subscription(self.toggle_wifi_ctr, conn.clone())
                    .map(Message::NetworkManagerEvent),
            ])
        } else {
            Subscription::batch(vec![timeline, network_sub])
        }
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}
