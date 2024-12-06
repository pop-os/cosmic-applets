use std::collections::HashSet;

use cosmic::{
    app,
    applet::{
        menu_button, menu_control_padding, padded_control,
        token::subscription::{activation_token_subscription, TokenRequest, TokenUpdate},
    },
    cctk::sctk::reexports::calloop,
    cosmic_theme::Spacing,
    iced::{
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        widget::{column, row},
        Alignment, Length, Subscription,
    },
    iced_runtime::core::{layout::Limits, window},
    iced_widget::Row,
    theme,
    widget::{
        button, container, divider,
        icon::{self, from_name},
        scrollable, text, text_input, Column,
    },
    Element, Task,
};
use cosmic_dbus_networkmanager::interface::enums::{
    ActiveConnectionState, DeviceState, NmConnectivityState,
};
use cosmic_time::{anim, chain, id, once_cell::sync::Lazy, Instant, Timeline};

use futures::channel::mpsc::UnboundedSender;
use zbus::Connection;

use crate::{
    config, fl,
    network_manager::{
        active_conns::active_conns_subscription, available_wifi::AccessPoint,
        current_networks::ActiveConnectionInfo, devices::devices_subscription,
        hw_address::HwAddress, network_manager_subscription,
        wireless_enabled::wireless_enabled_subscription, NetworkManagerEvent,
        NetworkManagerRequest, NetworkManagerState,
    },
};

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicNetworkApplet>(())
}

#[derive(Debug, Clone)]
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
    pub fn hw_address(&self) -> HwAddress {
        match self {
            Self::EnterPassword {
                access_point,
                password: _,
            } => access_point,
            Self::Waiting(ap) => ap,
            Self::Failure(ap) => ap,
        }
        .hw_address
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
    nm_state: NetworkManagerState,
    // UI state
    nm_sender: Option<UnboundedSender<NetworkManagerRequest>>,
    show_visible_networks: bool,
    new_connection: Option<NewConnectionState>,
    conn: Option<Connection>,
    timeline: Timeline,
    toggle_wifi_ctr: u128,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    failed_known_ssids: HashSet<String>,
    hw_device_to_show: Option<HwAddress>,
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
    fn update_nm_state(&mut self, mut new_state: NetworkManagerState) {
        self.update_togglers(&new_state);
        // check for failed conns that can be reset
        for new_s in &mut new_state.active_conns {
            let state = match new_s {
                ActiveConnectionInfo::WiFi { state, .. } => state,
                _ => continue,
            };

            if matches!(state, ActiveConnectionState::Activated) {
                self.failed_known_ssids.remove(&new_s.name());
                continue;
            }
            if matches!(
                state,
                ActiveConnectionState::Activating | ActiveConnectionState::Deactivating
            ) {
                continue;
            }

            if self.nm_state.active_conns.iter().any(|old_s| {
                matches!(
                    old_s,
                    ActiveConnectionInfo::WiFi {
                        state: ActiveConnectionState::Activating,
                        ..
                    } if new_s.name() == old_s.name()
                )
            }) {
                self.failed_known_ssids.insert(new_s.name());
            }
        }
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
    fn view_window_return<'a>(&self, mut content: Column<'a, Message>) -> Element<'a, Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        content = content
            .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]))
            .push(menu_button(text::body(fl!("settings"))).on_press(Message::OpenSettings));

        self.core
            .applet
            .popup_container(content.padding([8, 0, 8, 0]))
            .max_width(400.)
            .max_height(800.)
            .into()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    ActivateKnownWifi(String, HwAddress),
    Disconnect(String, HwAddress),
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
    Token(TokenUpdate),
    OpenSettings,
    ResetFailedKnownSsid(String, HwAddress),
    OpenHwDevice(Option<HwAddress>),
    // Errored(String),
}

impl cosmic::Application for CosmicNetworkApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        (
            Self {
                core,
                icon_name: "network-offline-symbolic".to_string(),
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

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::Frame(now) => self.timeline.now(now),
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    self.show_visible_networks = false;
                    return destroy_popup(p);
                } else {
                    // TODO request update of state maybe
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);
                    self.timeline = Timeline::new();

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
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
                    mut state,
                    success,
                    req,
                } => {
                    if let NetworkManagerRequest::SelectAccessPoint(ssid, hw_address) = &req {
                        let conn_match = self
                            .new_connection
                            .as_ref()
                            .map(|c| c.ssid() == ssid && c.hw_address() == *hw_address)
                            .unwrap_or_default();
                        if conn_match && success {
                            if let Some(s) =
                                state.active_conns.iter_mut().find(|ap| &ap.name() == ssid && ap.hw_address() == *hw_address)
                            {
                                match s {
                                    ActiveConnectionInfo::WiFi { state, .. } => {
                                        *state = ActiveConnectionState::Activated;
                                    }
                                    _ => {}
                                };
                            }
                            self.failed_known_ssids.remove(ssid);
                            self.new_connection = None;
                            self.show_visible_networks = false;
                        } else if !matches!(
                                &self.new_connection,
                                Some(NewConnectionState::EnterPassword { .. })
                            )
                        {
                            self.failed_known_ssids.insert(ssid.clone());
                        }
                    } else if let NetworkManagerRequest::Password(ssid, _, hw_address) = &req {
                        if let Some(NewConnectionState::Waiting(access_point)) =
                            self.new_connection.clone()
                        {
                            if !success && ssid == &access_point.ssid && *hw_address == access_point.hw_address {
                                self.new_connection =
                                    Some(NewConnectionState::Failure(access_point.clone()));
                            } else {
                                self.new_connection = None;
                                self.show_visible_networks = false;
                            }
                        } else if let Some(NewConnectionState::EnterPassword {
                            access_point, ..
                        }) = self.new_connection.clone()
                        {
                            if success && ssid == &access_point.ssid && *hw_address == access_point.hw_address {
                                self.new_connection = None;
                                self.show_visible_networks = false;
                            }
                        }
                    } else if self
                    .new_connection
                    .as_ref()
                    .map(|c| c.ssid()).is_some_and(|ssid| {
                        state.active_conns.iter().any(|c|
                            matches!(c, ActiveConnectionInfo::WiFi { name, state: ActiveConnectionState::Activated, .. } if ssid == name)
                        )
                    }) {
                        self.new_connection = None;
                        self.show_visible_networks = false;
                    }

                    if !matches!(req, NetworkManagerRequest::Reload)
                        && matches!(state.connectivity, NmConnectivityState::Portal)
                    {
                        let mut browser = std::process::Command::new("xdg-open");
                        browser.arg("http://204.pop-os.org/");

                        tokio::spawn(cosmic::process::spawn(browser));
                    }

                    self.update_nm_state(state);
                }
            },
            Message::SelectWirelessAccessPoint(access_point) => {
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    tx
                } else {
                    return Task::none();
                };

                let _ = tx.unbounded_send(NetworkManagerRequest::SelectAccessPoint(
                    access_point.ssid.clone(),
                    access_point.hw_address,
                ));

                self.new_connection = Some(NewConnectionState::EnterPassword {
                    access_point,
                    password: String::new(),
                });
            }
            Message::ToggleVisibleNetworks => {
                self.new_connection = None;
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
                    return Task::none();
                };

                if let Some(NewConnectionState::EnterPassword {
                    password,
                    access_point,
                }) = self.new_connection.take()
                {
                    let _ = tx.unbounded_send(NetworkManagerRequest::Password(
                        access_point.ssid.clone(),
                        password,
                        access_point.hw_address,
                    ));
                    self.new_connection
                        .replace(NewConnectionState::Waiting(access_point));
                };
            }
            Message::ActivateKnownWifi(ssid, hw_address) => {
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    if let Some(ap) = self
                        .nm_state
                        .known_access_points
                        .iter_mut()
                        .find(|c| c.ssid == ssid && c.hw_address == hw_address)
                    {
                        ap.working = true;
                    }
                    tx
                } else {
                    return Task::none();
                };
                let _ =
                    tx.unbounded_send(NetworkManagerRequest::SelectAccessPoint(ssid, hw_address));
            }
            Message::CancelNewConnection => {
                self.new_connection = None;
            }
            Message::Disconnect(ssid, hw_address) => {
                self.new_connection = None;
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    if let Some(ActiveConnectionInfo::WiFi { state, .. }) = self
                        .nm_state
                        .active_conns
                        .iter_mut()
                        .find(|c| c.name() == ssid && c.hw_address() == hw_address)
                    {
                        *state = ActiveConnectionState::Deactivating;
                    }
                    tx
                } else {
                    return Task::none();
                };
                let _ = tx.unbounded_send(NetworkManagerRequest::Disconnect(ssid, hw_address));
            }
            Message::CloseRequested(id) => {
                self.hw_device_to_show = None;
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
            Message::OpenSettings => {
                let exec = "cosmic-settings network".to_string();
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
                    cmd.arg("network");
                    if let Some(token) = token {
                        cmd.env("XDG_ACTIVATION_TOKEN", &token);
                        cmd.env("DESKTOP_STARTUP_ID", &token);
                    }
                    tokio::spawn(cosmic::process::spawn(cmd));
                }
            },
            Message::OpenHwDevice(hw_address) => self.hw_device_to_show = hw_address,
            Message::ResetFailedKnownSsid(ssid, hw_address) => {
                let ap = if let Some(pos) = self
                    .nm_state
                    .known_access_points
                    .iter()
                    .position(|ap| ap.ssid == ssid && ap.hw_address == hw_address)
                {
                    self.nm_state.known_access_points.remove(pos)
                } else if let Some((pos, ap)) = self
                    .nm_state
                    .active_conns
                    .iter()
                    .position(|conn| conn.name() == ssid && conn.hw_address() == hw_address)
                    .zip(
                        self.nm_state
                            .wireless_access_points
                            .iter()
                            .find(|ap| ap.ssid == ssid && ap.hw_address == hw_address),
                    )
                {
                    self.nm_state.active_conns.remove(pos);
                    ap.clone()
                } else {
                    tracing::warn!("Failed to find known access point with ssid: {}", ssid);
                    return Task::none();
                };
                if let Some(tx) = self.nm_sender.as_ref() {
                    let _ =
                        tx.unbounded_send(NetworkManagerRequest::Forget(ssid.clone(), hw_address));
                    self.show_visible_networks = true;
                    return self.update(Message::SelectWirelessAccessPoint(ap));
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Message> {
        self.core
            .applet
            .icon_button(&self.icon_name)
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let mut vpn_ethernet_col = column![];
        let mut known_wifi = Vec::new();
        for conn in &self.nm_state.active_conns {
            match conn {
                ActiveConnectionInfo::Vpn { name, ip_addresses } => {
                    if self.hw_device_to_show.is_some() {
                        continue;
                    }
                    let mut ipv4 = Vec::with_capacity(ip_addresses.len() + 1);
                    ipv4.push(text::body(name).into());
                    for addr in ip_addresses {
                        ipv4.push(text::caption(format!("{}: {}", fl!("ipv4"), addr)).into());
                    }
                    vpn_ethernet_col = vpn_ethernet_col.push(column![
                        row![
                            icon::icon(
                                icon::from_name(self.icon_name.clone())
                                    .symbolic(true)
                                    .into()
                            )
                            .size(40),
                            Column::with_children(ipv4),
                            text::body(fl!("connected"))
                                .width(Length::Fill)
                                .align_x(Alignment::End),
                        ]
                        .align_y(Alignment::Center)
                        .spacing(8)
                        .padding(menu_control_padding()),
                        padded_control(divider::horizontal::default())
                            .padding([space_xxs, space_s]),
                    ]);
                }
                ActiveConnectionInfo::Wired {
                    name,
                    hw_address,
                    speed,
                    ip_addresses,
                } => {
                    if self.hw_device_to_show.is_some()
                        && *hw_address != self.hw_device_to_show.unwrap()
                    {
                        continue;
                    }
                    let mut ipv4 = Vec::with_capacity(ip_addresses.len() + 1);
                    ipv4.push(text::body(name).into());
                    for addr in ip_addresses {
                        ipv4.push(text(format!("{}: {}", fl!("ipv4"), addr)).size(12).into());
                    }

                    vpn_ethernet_col = vpn_ethernet_col.push(column![
                        row![
                            icon::icon(
                                icon::from_name(self.icon_name.clone())
                                    .symbolic(true)
                                    .into()
                            )
                            .size(40),
                            Column::with_children(ipv4),
                            text::body(format!(
                                "{} - {speed} {}",
                                fl!("connected"),
                                fl!("megabits-per-second")
                            ))
                            .width(Length::Fill)
                            .align_x(Alignment::End),
                        ]
                        .align_y(Alignment::Center)
                        .spacing(8)
                        .padding(menu_control_padding()),
                        padded_control(divider::horizontal::default())
                            .padding([space_xxs, space_s]),
                    ]);
                }
                ActiveConnectionInfo::WiFi {
                    name,
                    ip_addresses,
                    state,
                    strength,
                    hw_address,
                } => {
                    if self.hw_device_to_show.is_some()
                        && hw_address != self.hw_device_to_show.as_ref().unwrap()
                    {
                        continue;
                    }
                    let mut ipv4 = Vec::with_capacity(ip_addresses.len());
                    for addr in ip_addresses {
                        ipv4.push(text(format!("{}: {}", fl!("ipv4"), addr)).size(12).into());
                    }
                    let mut btn_content = vec![
                        icon::from_name(wifi_icon(*strength))
                            .size(24)
                            .symbolic(true)
                            .into(),
                        column![text::body(name), Column::with_children(ipv4)]
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
                            text::body(fl!("connected"))
                                .align_x(Alignment::End)
                                .align_y(Alignment::Center)
                                .into(),
                        ),
                        _ => {}
                    };
                    if self.failed_known_ssids.contains(name) {
                        btn_content.push(
                            cosmic::widget::button::icon(
                                from_name("view-refresh-symbolic").size(16),
                            )
                            .icon_size(16)
                            .on_press(Message::ResetFailedKnownSsid(name.clone(), *hw_address))
                            .into(),
                        )
                    }

                    known_wifi.push(Element::from(
                        column![menu_button(
                            Row::with_children(btn_content)
                                .align_y(Alignment::Center)
                                .spacing(8)
                        )
                        .on_press(Message::Disconnect(name.clone(), *hw_address))]
                        .align_x(Alignment::Center),
                    ));
                }
            };
        }

        let mut content = if let Some(hw_device_to_show) = self.hw_device_to_show {
            column![
                vpn_ethernet_col,
                menu_button(row![
                    container(
                        icon::from_name("go-previous-symbolic")
                            .size(16)
                            .symbolic(true)
                    )
                    .align_x(Alignment::Start)
                    .align_y(Alignment::Center)
                    .width(Length::Fixed(24.0))
                    .height(Length::Fixed(24.0)),
                    text::body(hw_device_to_show.to_string())
                        .width(Length::Fill)
                        .height(Length::Fixed(24.0))
                        .align_y(Alignment::Center),
                ])
                .on_press(Message::OpenHwDevice(None))
            ]
        } else {
            column![
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
                padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
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
            ]
            .align_x(Alignment::Center)
        };

        if self.nm_state.airplane_mode {
            content = content.push(
                column!(
                    padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
                    icon::from_name("airplane-mode-symbolic")
                        .size(48)
                        .symbolic(true),
                    text::body(fl!("airplane-mode-on")),
                    text(fl!("turn-off-airplane-mode")).size(12)
                )
                .spacing(8)
                .padding([0, 0, 8, 0])
                .align_x(Alignment::Center)
                .width(Length::Fill),
            );
            return self.view_window_return(content);
        }

        if !self.nm_state.wifi_enabled {
            return self.view_window_return(content);
        }

        content = content
            .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));

        let wireless_hw_devices = self
            .nm_state
            .wireless_access_points
            .iter()
            .map(|ap| ap.hw_address)
            .collect::<std::collections::BTreeSet<_>>();

        if wireless_hw_devices.len() > 1 && self.hw_device_to_show.is_none() {
            for hw_device in wireless_hw_devices {
                let display_name = hw_device.to_string();

                let is_connected = self
                    .nm_state
                    .active_conns
                    .iter()
                    .any(|conn| conn.hw_address() == hw_device);
                let mut btn_content = vec![column![
                    text::body(display_name),
                    Column::with_children(vec![text("Adapter").size(10).into()])
                ]
                .width(Length::Fill)
                .into()];
                if is_connected {
                    btn_content.push(
                        text::body(fl!("connected"))
                            .width(Length::Fill)
                            .align_x(Alignment::End)
                            .into(),
                    );
                }
                btn_content.push(
                    icon::from_name("go-next-symbolic")
                        .size(16)
                        .symbolic(true)
                        .into(),
                );
                // todo figure our crash here. It happens if both this thing in content as well as
                // there is no new connection and we are viewing available networks(see insertion
                // of `list_col` into `content` on L1083)
                content = content.push(Element::from(
                    column![menu_button(
                        Row::with_children(btn_content)
                            .align_y(Alignment::Center)
                            .spacing(8)
                    )
                    .on_press(Message::OpenHwDevice(Some(hw_device.clone())))]
                    .align_x(Alignment::Center),
                ));
            }

            return self.view_window_return(content);
        }

        for known in &self.nm_state.known_access_points {
            if let Some(filter_hw_address) = self.hw_device_to_show {
                if filter_hw_address != known.hw_address {
                    continue;
                }
            }
            let mut btn_content = Vec::with_capacity(2);
            let ssid = text::body(&known.ssid).width(Length::Fill);
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

            if self.failed_known_ssids.contains(&known.ssid) {
                btn_content.push(
                    cosmic::widget::button::icon(from_name("view-refresh-symbolic").size(16))
                        .icon_size(16)
                        .on_press(Message::ResetFailedKnownSsid(
                            known.ssid.clone(),
                            known.hw_address,
                        ))
                        .into(),
                )
            }

            let mut btn = menu_button(
                Row::with_children(btn_content)
                    .align_y(Alignment::Center)
                    .spacing(8),
            );
            btn = match known.state {
                DeviceState::Failed
                | DeviceState::Unknown
                | DeviceState::Unmanaged
                | DeviceState::Disconnected
                | DeviceState::NeedAuth => btn.on_press(Message::ActivateKnownWifi(
                    known.ssid.clone(),
                    known.hw_address,
                )),
                DeviceState::Activated => {
                    btn.on_press(Message::Disconnect(known.ssid.clone(), known.hw_address))
                }
                _ => btn,
            };
            known_wifi.push(Element::from(row![btn].align_y(Alignment::Center)));
        }
        let has_known_wifi = !known_wifi.is_empty();
        content = content.push(Column::with_children(known_wifi));
        if has_known_wifi {
            content = content
                .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));
        }

        let dropdown_icon = if self.show_visible_networks {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };
        let available_connections_btn = menu_button(row![
            text::body(fl!("visible-wireless-networks"))
                .width(Length::Fill)
                .height(Length::Fixed(24.0))
                .align_y(Alignment::Center),
            container(icon::from_name(dropdown_icon).size(16).symbolic(true))
                .center(Length::Fixed(24.0))
        ])
        .on_press(Message::ToggleVisibleNetworks);
        content = content.push(available_connections_btn);

        if !self.show_visible_networks {
            return self.view_window_return(content);
        }

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
                            text::body(&access_point.ssid),
                        ]
                        .align_y(Alignment::Center)
                        .spacing(12),
                    );
                    content = content.push(id);
                    let col = padded_control(
                        column![
                            text::body(fl!("enter-password")),
                            text_input("", password)
                                .on_input(Message::Password)
                                .on_paste(Message::Password)
                                .on_submit(Message::SubmitPassword)
                                .password(),
                            container(text::body(fl!("router-wps-button"))).padding(8),
                            row![
                                button::standard(fl!("cancel"))
                                    .on_press(Message::CancelNewConnection),
                                button::suggested(fl!("connect")).on_press(Message::SubmitPassword)
                            ]
                            .spacing(24)
                        ]
                        .spacing(8)
                        .align_x(Alignment::Center),
                    )
                    .align_x(Alignment::Center);
                    content = content.push(col);
                }
                NewConnectionState::Waiting(access_point) => {
                    let id = row![
                        icon::from_name("network-wireless-acquiring-symbolic")
                            .size(24)
                            .symbolic(true),
                        text::body(&access_point.ssid),
                    ]
                    .align_y(Alignment::Center)
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
                            text::body(&access_point.ssid),
                        ]
                        .align_y(Alignment::Center)
                        .spacing(12),
                    )
                    .align_x(Alignment::Center);
                    content = content.push(id);
                    let col = padded_control(
                        column![
                            text(fl!("unable-to-connect")),
                            text(fl!("check-wifi-connection")),
                            row![
                                button::standard(fl!("cancel"))
                                    .on_press(Message::CancelNewConnection),
                                button::suggested(fl!("connect")).on_press(
                                    Message::SelectWirelessAccessPoint(access_point.clone())
                                )
                            ]
                            .spacing(24)
                        ]
                        .spacing(16)
                        .align_x(Alignment::Center),
                    )
                    .align_x(Alignment::Center);
                    content = content.push(col);
                }
            }
        } else {
            let mut list_col = Vec::with_capacity(self.nm_state.wireless_access_points.len());
            for ap in &self.nm_state.wireless_access_points {
                if ap.hw_address != self.hw_device_to_show.unwrap_or(ap.hw_address) {
                    continue;
                }
                if self
                    .nm_state
                    .active_conns
                    .iter()
                    .any(|a| ap.ssid == a.name() && ap.hw_address == a.hw_address())
                {
                    continue;
                }
                let button = menu_button(
                    row![
                        icon::from_name(wifi_icon(ap.strength))
                            .size(16)
                            .symbolic(true),
                        text::body(&ap.ssid).align_y(Alignment::Center)
                    ]
                    .align_y(Alignment::Center)
                    .spacing(12),
                )
                .on_press(Message::SelectWirelessAccessPoint(ap.clone()));
                list_col.push(button.into());
            }
            // todo fixup crash that happens if both content gets update here and with such
            // condition `if wireless_hw_devices.len() > 1 && self.hw_device_to_show.is_none()`
            // See reference to it on L843
            content = content
                .push(scrollable(Column::with_children(list_col)).height(Length::Fixed(300.0)));
        }

        self.view_window_return(content)
    }

    fn subscription(&self) -> Subscription<Message> {
        let network_sub = network_manager_subscription(0).map(Message::NetworkManagerEvent);
        let timeline = self
            .timeline
            .as_subscription()
            .map(|(_, now)| Message::Frame(now));
        let token_sub = activation_token_subscription(0).map(Message::Token);

        if let Some(conn) = self.conn.as_ref() {
            let has_popup = self.popup.is_some();
            Subscription::batch(vec![
                timeline,
                network_sub,
                token_sub,
                active_conns_subscription(self.toggle_wifi_ctr, conn.clone())
                    .map(Message::NetworkManagerEvent),
                devices_subscription(self.toggle_wifi_ctr, has_popup, conn.clone())
                    .map(Message::NetworkManagerEvent),
                wireless_enabled_subscription(self.toggle_wifi_ctr, conn.clone())
                    .map(Message::NetworkManagerEvent),
            ])
        } else {
            Subscription::batch(vec![timeline, network_sub, token_sub])
        }
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}
