use anyhow::Context;
use cosmic_dbus_networkmanager::settings::{NetworkManagerSettings, connection::Settings};
use cosmic_settings_network_manager_subscription::{
    self as network_manager, NetworkManagerState, UUID,
    active_conns::active_conns_subscription,
    available_wifi::{AccessPoint, NetworkType},
    current_networks::ActiveConnectionInfo,
    hw_address::HwAddress,
    nm_secret_agent::{self, PasswordFlag, SecretSender},
};
use indexmap::IndexMap;
use rustc_hash::FxHashSet;
use secure_string::SecureString;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, LazyLock},
    time::Duration,
};

use cosmic::{
    Apply, Element, Task, app,
    applet::{
        menu_button, menu_control_padding, padded_control,
        token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    },
    cctk::sctk::reexports::calloop,
    cosmic_theme::Spacing,
    iced::{
        Alignment, Length, Subscription,
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        widget::{column, row},
    },
    iced_runtime::core::window,
    surface, theme,
    widget::{
        Column, Row, button, container, divider,
        icon::{self, from_name},
        scrollable, secure_input, text, text_input,
    },
};
use cosmic_dbus_networkmanager::interface::{
    access_point,
    enums::{ActiveConnectionState, DeviceState, NmConnectivityState, NmState},
};
use cosmic_time::{Instant, Timeline, anim, chain, id};

use futures::{StreamExt, channel::mpsc::TrySendError};
use zbus::{Connection, zvariant::ObjectPath};

use crate::{config, fl};

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicNetworkApplet>(())
}

#[derive(Debug, Clone)]
enum NewConnectionState {
    EnterPassword {
        access_point: AccessPoint,
        description: Option<String>,
        identity: String,
        password: SecureString,
        password_hidden: bool,
    },
    Waiting(AccessPoint),
    Failure(AccessPoint),
}

impl NewConnectionState {
    pub fn ssid(&self) -> &str {
        &match self {
            Self::EnterPassword { access_point, .. } => access_point,
            Self::Waiting(ap) => ap,
            Self::Failure(ap) => ap,
        }
        .ssid
    }
    pub fn hw_address(&self) -> HwAddress {
        match self {
            Self::EnterPassword { access_point, .. } => access_point,
            Self::Waiting(ap) => ap,
            Self::Failure(ap) => ap,
        }
        .hw_address
    }
}

impl From<NewConnectionState> for AccessPoint {
    fn from(connection_state: NewConnectionState) -> Self {
        match connection_state {
            NewConnectionState::EnterPassword { access_point, .. } => access_point,
            NewConnectionState::Waiting(access_point) => access_point,
            NewConnectionState::Failure(access_point) => access_point,
        }
    }
}

static WIFI: LazyLock<id::Toggler> = LazyLock::new(id::Toggler::unique);
static AIRPLANE_MODE: LazyLock<id::Toggler> = LazyLock::new(id::Toggler::unique);

#[derive(Default, Debug, Clone)]
pub struct MyNetworkState {
    pub known_vpns: IndexMap<UUID, ConnectionSettings>,
    pub ssid_to_uuid: BTreeMap<Box<str>, Box<str>>,
    pub devices: Vec<Arc<network_manager::devices::DeviceInfo>>,
    pub password: Option<Password>,
    pub connecting: BTreeSet<network_manager::SSID>,
    pub nm_state: NetworkManagerState,
    pub requested_vpn: Option<RequestedVpn>,
}

#[derive(Debug, Clone)]
pub struct RequestedVpn {
    name: String,
    uuid: Arc<str>,
    description: Option<String>,
    password: SecureString,
    password_hidden: bool,
    tx: SecretSender,
}

#[derive(Clone, Debug)]
pub enum ConnectionSettings {
    Vpn(VpnConnectionSettings),
    Wireguard { id: String },
}

#[derive(Clone, Debug, Default)]
pub struct VpnConnectionSettings {
    id: String,
    username: Option<String>,
    connection_type: Option<ConnectionType>,
    password_flag: Option<PasswordFlag>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ConnectionType {
    Password,
}

impl VpnConnectionSettings {
    fn password_flag(&self) -> Option<PasswordFlag> {
        self.connection_type
            .as_ref()
            .is_some_and(|ct| match ct {
                ConnectionType::Password => true,
            })
            .then_some(self.password_flag)
            .flatten()
    }
}

#[derive(Default)]
struct CosmicNetworkApplet {
    core: cosmic::app::Core,
    icon_name: String,
    popup: Option<window::Id>,

    // NM state
    nm_sender: Option<futures::channel::mpsc::UnboundedSender<network_manager::Request>>,
    nm_task: Option<tokio::sync::oneshot::Sender<()>>,
    secret_tx: Option<tokio::sync::mpsc::Sender<nm_secret_agent::Request>>,
    nm_state: MyNetworkState,

    // UI state
    show_visible_networks: bool,
    show_available_vpns: bool,
    new_connection: Option<NewConnectionState>,
    conn: Option<Connection>,
    timeline: Timeline,
    toggle_wifi_ctr: u128,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    failed_known_ssids: FxHashSet<Arc<str>>,

    /// When defined, displays connections for the specific device.
    active_device: Option<Arc<network_manager::devices::DeviceInfo>>,
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

fn vpn_section<'a>(
    nm_state: &'a MyNetworkState,
    show_available_vpns: bool,
    space_xxs: u16,
    space_s: u16,
) -> Column<'a, Message> {
    let mut vpn_col = column![];

    if !nm_state.known_vpns.is_empty() {
        let dropdown_icon = if show_available_vpns {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };

        vpn_col = vpn_col
            .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));

        if let Some(requested_vpn) = nm_state.requested_vpn.as_ref() {
            let column_content = vec![
                text::body(
                    requested_vpn
                        .description
                        .as_deref()
                        .unwrap_or(requested_vpn.uuid.as_ref()),
                )
                .width(Length::Fill)
                .into(),
                secure_input(
                    "",
                    Cow::Borrowed(requested_vpn.password.unsecure()),
                    Some(Message::ToggleVPNPasswordVisibility),
                    requested_vpn.password_hidden,
                )
                .on_input(|s| Message::VPNPasswordUpdate(s.into()))
                .on_paste(|s| Message::VPNPasswordUpdate(s.into()))
                .on_submit(|_| Message::ConnectVPNWithPassword)
                .width(Length::Fill)
                .into(),
                row![
                    button::standard(fl!("cancel")).on_press(Message::CancelVPNConnection),
                    button::suggested(fl!("connect")).on_press(Message::ConnectVPNWithPassword)
                ]
                .spacing(24)
                .into(),
            ];
            let col = padded_control(
                Column::with_children(column_content)
                    .spacing(8)
                    .align_x(Alignment::Center),
            )
            .align_x(Alignment::Center);
            vpn_col = vpn_col.push(col);
        }

        let vpn_toggle_btn = menu_button(row![
            text::body(fl!("vpn-connections"))
                .width(Length::Fill)
                .height(Length::Fixed(24.0))
                .align_y(Alignment::Center),
            container(icon::from_name(dropdown_icon).size(16).symbolic(true))
                .center(Length::Fixed(24.0))
        ])
        .on_press(Message::ToggleVpnList);

        vpn_col = vpn_col.push(vpn_toggle_btn);

        if show_available_vpns {
            for (uuid, connection) in &nm_state.known_vpns {
                let id = match connection {
                    ConnectionSettings::Vpn(connection) => connection.id.as_str(),
                    ConnectionSettings::Wireguard { id } => id.as_str(),
                };
                // Check if this VPN is currently active
                let is_active = nm_state.nm_state.active_conns.iter().any(
                    |conn| matches!(conn, ActiveConnectionInfo::Vpn { name, .. } if name == id),
                );

                let mut btn_content = vec![
                    icon::from_name("network-vpn-symbolic")
                        .size(24)
                        .symbolic(true)
                        .into(),
                    text::body(id).width(Length::Fill).into(),
                ];

                if is_active {
                    btn_content.push(text::body(fl!("connected")).align_x(Alignment::End).into());
                }

                let mut btn = menu_button(
                    Row::with_children(btn_content)
                        .align_y(Alignment::Center)
                        .spacing(8),
                );

                btn = if is_active {
                    btn.on_press(Message::DeactivateVpn(uuid.clone()))
                } else {
                    btn.on_press(Message::ActivateVpn(uuid.clone()))
                };

                vpn_col = vpn_col.push(btn);
            }
        }
    }

    vpn_col
}

impl CosmicNetworkApplet {
    fn update_nm_state(&mut self, mut new_state: NetworkManagerState) {
        self.update_togglers(&new_state);
        // check for failed conns that can be reset
        for new_s in &mut new_state.active_conns {
            let ActiveConnectionInfo::WiFi { state, .. } = new_s else {
                continue;
            };

            if matches!(state, ActiveConnectionState::Activated) {
                self.failed_known_ssids.remove(new_s.name().as_str());
                continue;
            }
            if matches!(
                state,
                ActiveConnectionState::Activating | ActiveConnectionState::Deactivating
            ) {
                continue;
            }

            if self.nm_state.nm_state.active_conns.iter().any(|old_s| {
                matches!(
                    old_s,
                    ActiveConnectionInfo::WiFi {
                        state: ActiveConnectionState::Activating,
                        ..
                    } if new_s.name() == old_s.name()
                )
            }) {
                self.failed_known_ssids.insert(new_s.name().into());
            }
        }
        self.nm_state.nm_state = new_state;
        self.update_icon_name();
    }

    fn update_icon_name(&mut self) {
        self.icon_name = self
            .nm_state
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
            .to_string();
    }

    fn update_togglers(&mut self, state: &NetworkManagerState) {
        let timeline = &mut self.timeline;
        let mut changed = false;
        if self.nm_state.nm_state.wifi_enabled != state.wifi_enabled {
            self.nm_state.nm_state.wifi_enabled = state.wifi_enabled;
            changed = true;
            let chain = if state.wifi_enabled {
                chain::Toggler::on(WIFI.clone(), 1.)
            } else {
                chain::Toggler::off(WIFI.clone(), 1.)
            };
            timeline.set_chain(chain);
        }

        if self.nm_state.nm_state.airplane_mode != state.airplane_mode {
            self.nm_state.nm_state.airplane_mode = state.airplane_mode;
            changed = true;
            let chain = if state.airplane_mode {
                chain::Toggler::on(AIRPLANE_MODE.clone(), 1.)
            } else {
                chain::Toggler::off(AIRPLANE_MODE.clone(), 1.)
            };
            timeline.set_chain(chain);
        }
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
            .into()
    }

    fn connect_vpn(&mut self, uuid: Arc<str>) -> Task<cosmic::Action<Message>> {
        if let Some((tx, conn)) = self.nm_sender.clone().zip(self.conn.clone()) {
            cosmic::task::future(async move {
                // Find the connection by UUID
                if let Ok(nm_settings) = NetworkManagerSettings::new(&conn).await {
                    if let Ok(connections) = nm_settings.list_connections().await {
                        for connection in connections {
                            if let Ok(settings) = connection.get_settings().await {
                                let settings = Settings::new(settings);
                                if let Some(conn_settings) = &settings.connection {
                                    if conn_settings.uuid.as_ref().is_some_and(|conn_uuid| {
                                        conn_uuid.as_str() == uuid.as_ref()
                                    }) {
                                        let path = connection.inner().path().clone().to_owned();
                                        if let Err(err) =
                                            tx.unbounded_send(network_manager::Request::Activate(
                                                ObjectPath::try_from("/").unwrap(),
                                                path,
                                            ))
                                        {
                                            if err.is_disconnected() {
                                                return zbus::Connection::system()
                                                    .await
                                                    .context(
                                                        "failed to create system dbus connection",
                                                    )
                                                    .map_or_else(
                                                        |why| Message::Error(why.to_string()),
                                                        Message::NetworkManagerConnect,
                                                    );
                                            }

                                            tracing::error!("{err:?}");
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Message::Refresh
            })
        } else {
            tracing::warn!("No sender available to activate VPN.");
            Task::none()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    ToggleAirplaneMode(bool),
    ToggleVisibleNetworks,
    SelectWirelessAccessPoint(AccessPoint),
    CancelNewConnection,
    Frame(Instant),
    Token(TokenUpdate),
    OpenSettings,
    ResetFailedKnownSsid(String, HwAddress),
    TogglePasswordVisibility,
    Surface(surface::Action),
    ActivateVpn(Arc<str>),   // UUID of VPN to activate
    DeactivateVpn(Arc<str>), // UUID of VPN to deactivate
    ToggleVpnList,           // Show/hide available VPNs
    /// An update from the secret agent
    SecretAgent(network_manager::nm_secret_agent::Event),
    /// Connect to a WiFi network access point.
    Connect(network_manager::SSID, HwAddress),
    /// Connect with a password
    ConnectWithPassword,
    KnownConnections(IndexMap<UUID, ConnectionSettings>),
    /// Settings for known connections.
    ConnectionSettings(BTreeMap<Box<str>, Box<str>>),
    /// Disconnect from an access point.
    Disconnect(network_manager::SSID, HwAddress),
    /// An error occurred.
    Error(String),
    /// Identity update from the dialog
    IdentityUpdate(String),
    /// An update from the network manager daemon
    NetworkManager(network_manager::Event),
    /// Successfully connected to the system dbus.
    NetworkManagerConnect(zbus::Connection),
    /// Update the password from the dialog
    PasswordUpdate(SecureString),
    /// Update NetworkManagerState
    UpdateState(NetworkManagerState),
    /// Update the devices lists
    UpdateDevices(Vec<network_manager::devices::DeviceInfo>),
    /// Toggle WiFi access
    WiFiEnable(bool),
    /// Refresh state
    Refresh,
    ToggleVPNPasswordVisibility,
    ConnectVPNWithPassword,
    VPNPasswordUpdate(SecureString),
    CancelVPNConnection,
    /// Selects a device to display connections from
    SelectDevice(Option<Arc<network_manager::devices::DeviceInfo>>),
}

#[derive(Debug, Clone)]
struct Password {
    ssid: network_manager::SSID,
    hw_address: HwAddress,
    identity: Option<String>,
    password: SecureString,
    password_hidden: bool,
    tx: SecretSender,
}

fn connection_settings(conn: zbus::Connection) -> Task<Message> {
    let settings = async move {
        let settings = network_manager::dbus::settings::NetworkManagerSettings::new(&conn).await?;

        _ = settings.load_connections(&[]).await;

        let settings = settings
            // Get a list of known connections.
            .list_connections()
            .await?
            // Prepare for wrapping in a concurrent stream.
            .into_iter()
            .map(|conn| async move { conn })
            // Create a concurrent stream for each connection.
            .apply(futures::stream::FuturesOrdered::from_iter)
            // Concurrently fetch settings for each connection.
            .filter_map(|conn| async move {
                conn.get_settings()
                    .await
                    .map(network_manager::Settings::new)
                    .ok()
            })
            // Reduce the settings list into a SSID->UUID map.
            .fold(BTreeMap::new(), |mut set, settings| async move {
                if let Some(ref wifi) = settings.wifi
                    && let Some(ssid) = wifi
                        .ssid
                        .clone()
                        .and_then(|ssid| String::from_utf8(ssid).ok())
                    && let Some(ref connection) = settings.connection
                    && let Some(uuid) = connection.uuid.clone()
                {
                    set.insert(ssid.into(), uuid.into());
                    return set;
                }

                set
            })
            .await;

        Ok::<_, zbus::Error>(settings)
    };

    cosmic::task::future(async move {
        settings
            .await
            .context("failed to get connection settings")
            .map_or_else(
                |why| Message::Error(why.to_string()),
                Message::ConnectionSettings,
            )
    })
}

pub fn update_state(conn: zbus::Connection) -> Task<Message> {
    cosmic::task::future(async move {
        match NetworkManagerState::new(&conn).await {
            Ok(state) => Message::UpdateState(state),
            Err(why) => Message::Error(why.to_string()),
        }
    })
}

pub fn update_devices(conn: zbus::Connection) -> Task<Message> {
    cosmic::task::future(async move {
        let filter =
            |device_type| matches!(device_type, network_manager::devices::DeviceType::Wifi);
        match network_manager::devices::list(&conn, filter).await {
            Ok(devices) => Message::UpdateDevices(devices),
            Err(why) => Message::Error(why.to_string()),
        }
    })
}

impl CosmicNetworkApplet {
    fn connect(&mut self, conn: zbus::Connection) -> Task<Message> {
        if self.nm_task.is_none() {
            let popup = self.popup;
            let (canceller, task) = crate::utils::forward_event_loop(move |emitter| async move {
                let (tx, mut rx) = futures::channel::mpsc::channel(1);

                if popup.is_some() {
                    let watchers = std::pin::pin!(async move {
                        futures::join!(
                            network_manager::watch(conn.clone(), tx.clone()),
                            network_manager::active_conns::watch(conn.clone(), tx.clone(),),
                            network_manager::wireless_enabled::watch(conn.clone(), tx.clone()),
                            network_manager::watch_connections_changed(conn, tx,)
                        );
                    });
                    let forwarder = std::pin::pin!(async move {
                        while let Some(message) = rx.next().await {
                            _ = emitter.emit(Message::NetworkManager(message)).await;
                        }
                    });

                    futures::future::select(watchers, forwarder).await;
                } else {
                    let watchers = std::pin::pin!(async move {
                        futures::join!(
                            network_manager::watch(conn.clone(), tx.clone()),
                            network_manager::active_conns::watch(conn.clone(), tx.clone(),),
                            network_manager::wireless_enabled::watch(conn.clone(), tx.clone()),
                        );
                    });
                    let forwarder = std::pin::pin!(async move {
                        while let Some(message) = rx.next().await {
                            _ = emitter.emit(Message::NetworkManager(message)).await;
                        }
                    });

                    futures::future::select(watchers, forwarder).await;
                };
            });

            self.nm_task = Some(canceller);
            return task.map(Message::from);
        }

        Task::none()
    }
}

fn load_vpns(conn: zbus::Connection) -> Task<crate::app::Message> {
    let settings = async move {
        let settings = network_manager::dbus::settings::NetworkManagerSettings::new(&conn).await?;

        _ = settings.load_connections(&[]).await;

        let settings = settings
            // Get a list of known connections.
            .list_connections()
            .await?
            // Prepare for wrapping in a concurrent stream.
            .into_iter()
            .map(|conn| async move { conn })
            // Create a concurrent stream for each connection.
            .apply(futures::stream::FuturesOrdered::from_iter)
            // Concurrently fetch settings for each connection, and filter for VPN.
            .filter_map(|conn| async move {
                let settings = conn.get_settings().await.ok()?;

                let connection = settings.get("connection")?;

                match connection
                    .get("type")?
                    .downcast_ref::<String>()
                    .ok()?
                    .as_str()
                {
                    "vpn" => (),

                    "wireguard" => {
                        let id = connection.get("id")?.downcast_ref::<String>().ok()?;
                        let uuid = connection.get("uuid")?.downcast_ref::<String>().ok()?;
                        return Some((Arc::from(uuid), ConnectionSettings::Wireguard { id }));
                    }

                    _ => return None,
                }

                let vpn = settings.get("vpn")?;
                let id = connection.get("id")?.downcast_ref::<String>().ok()?;
                let uuid = connection.get("uuid")?.downcast_ref::<String>().ok()?;

                let (connection_type, username, password_flag) = vpn
                    .get("data")
                    .and_then(|data| data.downcast_ref::<zbus::zvariant::Dict>().ok())
                    .map(|dict| {
                        let (mut connection_type, mut password_flag) = (None, None);
                        let mut username = vpn
                            .get("user-name")
                            .and_then(|u| u.downcast_ref::<String>().ok());
                        if dict
                            .get::<String, String>(&String::from("connection-type"))
                            .ok()
                            .flatten()
                            .as_deref()
                            // may be "password" or "password-tls"
                            .is_some_and(|p| p.starts_with("password"))
                        {
                            connection_type = Some(ConnectionType::Password);
                            username = Some(username.unwrap_or_default());

                            password_flag = dict
                                .get::<String, String>(&String::from("password-flags"))
                                .ok()
                                .flatten()
                                .and_then(|value| match value.as_str() {
                                    "0" => Some(PasswordFlag::None),
                                    "1" => Some(PasswordFlag::AgentOwned),
                                    "2" => Some(PasswordFlag::NotSaved),
                                    "4" => Some(PasswordFlag::NotRequired),
                                    _ => None,
                                });
                        }

                        (connection_type, username, password_flag)
                    })
                    .unwrap_or_default();

                Some((
                    Arc::from(uuid),
                    ConnectionSettings::Vpn(VpnConnectionSettings {
                        id,
                        connection_type,
                        password_flag,
                        username,
                    }),
                ))
            })
            // Reduce the settings list into
            .fold(IndexMap::new(), |mut set, (uuid, data)| async move {
                set.insert(uuid, data);
                set
            })
            .await;

        Ok::<_, zbus::Error>(settings)
    };

    cosmic::task::future(async move {
        settings.await.map_or_else(
            |why| Message::Error(why.to_string()),
            Message::KnownConnections,
        )
    })
}

fn system_conn() -> Task<Message> {
    cosmic::Task::future(async move {
        zbus::Connection::system()
            .await
            .context("failed to create system dbus connection")
            .map_or_else(
                |why| Message::Error(why.to_string()),
                Message::NetworkManagerConnect,
            )
    })
}

impl cosmic::Application for CosmicNetworkApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        let mut applet = Self {
            core,
            icon_name: "network-wired-disconnected-symbolic".to_string(),
            token_tx: None,
            ..Default::default()
        };

        (applet, system_conn().map(cosmic::Action::App))
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
                    let mut tasks = Vec::with_capacity(2);
                    if let Some(conn) = self.conn.clone() {
                        tasks.push(update_state(conn.clone()));
                        tasks.push(update_devices(conn.clone()));
                        tasks.push(load_vpns(conn));
                        let (tx, rx) = tokio::sync::mpsc::channel(4);
                        self.secret_tx = Some(tx);
                        let my_id = format!(
                            "com.system76.CosmicSettings.Applet.{}.NetworkManager.SecretAgent",
                            uuid::Uuid::new_v4()
                        );
                        tasks.push(
                            cosmic::Task::stream(nm_secret_agent::secret_agent_stream(
                                my_id.clone(),
                                rx,
                            ))
                            .map(Message::SecretAgent),
                        );
                    }
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

                    tasks.push(system_conn());
                    tasks.push(get_popup(popup_settings));

                    return Task::batch(tasks).map(cosmic::Action::App);
                }
            }
            Message::ToggleAirplaneMode(enabled) => {
                self.toggle_wifi_ctr += 1;
                if let Some(tx) = self.nm_sender.as_mut() {
                    if let Err(err) =
                        tx.unbounded_send(network_manager::Request::SetAirplaneMode(enabled))
                    {
                        if err.is_disconnected() {
                            return system_conn().map(cosmic::Action::App);
                        }

                        tracing::error!("{err:?}");
                    }
                }
            }
            Message::SelectWirelessAccessPoint(access_point) => {
                let Some(tx) = self.nm_sender.as_ref() else {
                    return Task::none();
                };

                if matches!(access_point.network_type, NetworkType::Open) {
                    if let Err(err) =
                        tx.unbounded_send(network_manager::Request::SelectAccessPoint(
                            access_point.ssid.clone(),
                            access_point.network_type,
                            self.secret_tx.clone(),
                            self.active_device.as_ref().map(|d| d.interface.clone()),
                        ))
                    {
                        if err.is_disconnected() {
                            return system_conn().map(cosmic::Action::App);
                        }

                        tracing::error!("{err:?}");
                    }
                    self.new_connection = Some(NewConnectionState::Waiting(access_point));
                } else {
                    if self
                        .nm_state
                        .nm_state
                        .known_access_points
                        .contains(&access_point)
                    {
                        if let Err(err) =
                            tx.unbounded_send(network_manager::Request::SelectAccessPoint(
                                access_point.ssid.clone(),
                                access_point.network_type,
                                self.secret_tx.clone(),
                                self.active_device.as_ref().map(|d| d.interface.clone()),
                            ))
                        {
                            if err.is_disconnected() {
                                return system_conn().map(cosmic::Action::App);
                            }

                            tracing::error!("{err:?}");
                        }
                    }
                    self.new_connection = Some(NewConnectionState::EnterPassword {
                        access_point,
                        description: None,
                        identity: String::new(),
                        password: String::new().into(),
                        password_hidden: true,
                    });
                }
            }
            Message::ToggleVisibleNetworks => {
                self.new_connection = None;
                self.show_visible_networks = !self.show_visible_networks;
            }
            Message::TogglePasswordVisibility => {
                if let Some(NewConnectionState::EnterPassword {
                    password_hidden, ..
                }) = &mut self.new_connection
                {
                    *password_hidden = !*password_hidden;
                }
            }
            Message::CancelNewConnection => {
                self.new_connection = None;
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                    if let Some(cancel) = self.nm_task.take() {
                        _ = cancel.send(());
                    }

                    self.secret_tx = None;
                    return system_conn().map(cosmic::Action::App);
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
            Message::SelectDevice(device) => {
                self.active_device = device;
            }
            Message::ResetFailedKnownSsid(ssid, hw_address) => {
                let ap = if let Some(pos) = self
                    .nm_state
                    .nm_state
                    .known_access_points
                    .iter()
                    .position(|ap| ap.ssid.as_ref() == ssid.as_str() && ap.hw_address == hw_address)
                {
                    self.nm_state.nm_state.known_access_points.remove(pos)
                } else if let Some((pos, ap)) = self
                    .nm_state
                    .nm_state
                    .active_conns
                    .iter()
                    .position(|conn| {
                        conn.name() == ssid && active_conn_hw_address(conn) == hw_address
                    })
                    .zip(
                        self.nm_state
                            .nm_state
                            .wireless_access_points
                            .iter()
                            .find(|ap| {
                                ap.ssid.as_ref() == ssid.as_str() && ap.hw_address == hw_address
                            }),
                    )
                {
                    self.nm_state.nm_state.active_conns.remove(pos);
                    ap.clone()
                } else {
                    tracing::warn!("Failed to find known access point with ssid: {}", ssid);
                    return Task::none();
                };
                if let Some(tx) = self.nm_sender.as_ref() {
                    if let Err(err) =
                        tx.unbounded_send(network_manager::Request::Forget(ssid.into()))
                    {
                        if err.is_disconnected() {
                            return system_conn().map(cosmic::Action::App);
                        }

                        tracing::error!("{err:?}");
                    }
                    self.show_visible_networks = true;
                    return self.update(Message::SelectWirelessAccessPoint(ap));
                }
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Message::ActivateVpn(uuid) => {
                return self.connect_vpn(uuid.clone());
            }
            Message::DeactivateVpn(name) => {
                if let Some(tx) = self.nm_sender.as_ref() {
                    if let Err(err) = tx.unbounded_send(network_manager::Request::Deactivate(name))
                    {
                        if err.is_disconnected() {
                            return system_conn().map(cosmic::Action::App);
                        }

                        tracing::error!("{err:?}");
                    }
                }
            }
            Message::ToggleVpnList => {
                self.show_available_vpns = !self.show_available_vpns;
            }
            Message::Connect(ssid, hw_address) => {
                let mut network_type = NetworkType::Open;
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    if let Some(ap) = self
                        .nm_state
                        .nm_state
                        .known_access_points
                        .iter_mut()
                        .find(|c| c.ssid == ssid && c.hw_address == hw_address)
                    {
                        network_type = ap.network_type;
                        ap.working = true;
                    }
                    tx
                } else {
                    return Task::none();
                };
                if let Err(err) = tx.unbounded_send(network_manager::Request::SelectAccessPoint(
                    ssid,
                    network_type,
                    self.secret_tx.clone(),
                    self.active_device.as_ref().map(|d| d.interface.clone()),
                )) {
                    if err.is_disconnected() {
                        return system_conn().map(cosmic::Action::App);
                    }

                    tracing::error!("{err:?}");
                }
            }
            Message::ConnectWithPassword => {
                // save password
                let Some(tx) = self.nm_sender.as_ref() else {
                    return Task::none();
                };

                if let Some(NewConnectionState::EnterPassword {
                    password,
                    access_point,
                    identity,
                    ..
                }) = self.new_connection.take()
                {
                    let is_enterprise: bool = matches!(access_point.network_type, NetworkType::EAP);

                    if let Err(err) = tx.unbounded_send(network_manager::Request::Authenticate {
                        ssid: access_point.ssid.to_string(),
                        identity: is_enterprise.then(|| identity.clone()),
                        password,
                        secret_tx: self.secret_tx.clone(),
                        interface: self.active_device.as_ref().map(|d| d.interface.clone()),
                    }) {
                        if err.is_disconnected() {
                            return system_conn().map(cosmic::Action::App);
                        }
                        tracing::error!("Failed to authenticate with network manager");
                    }
                    self.new_connection
                        .replace(NewConnectionState::Waiting(access_point));
                }
            }
            Message::ConnectionSettings(btree_map) => {
                self.nm_state.ssid_to_uuid = btree_map;
            }
            Message::Disconnect(ssid, hw_address) => {
                self.new_connection = None;
                let tx = if let Some(tx) = self.nm_sender.as_ref() {
                    if let Some(ActiveConnectionInfo::WiFi { state, .. }) =
                        self.nm_state.nm_state.active_conns.iter_mut().find(|c| {
                            let c_hw_address = match c {
                                ActiveConnectionInfo::Wired { hw_address, .. }
                                | ActiveConnectionInfo::WiFi { hw_address, .. } => {
                                    HwAddress::from_str(hw_address).unwrap()
                                }
                                ActiveConnectionInfo::Vpn { .. } => HwAddress::default(),
                            };
                            c.name().as_str() == ssid.as_ref() && c_hw_address == hw_address
                        })
                    {
                        *state = ActiveConnectionState::Deactivating;
                    }
                    tx
                } else {
                    return Task::none();
                };
                if let Err(err) = tx.unbounded_send(network_manager::Request::Disconnect(ssid)) {
                    if err.is_disconnected() {
                        return system_conn().map(cosmic::Action::App);
                    }

                    tracing::error!("{err:?}");
                }
            }
            Message::Error(error) => {
                tracing::error!("error: {error:?}")
            }
            Message::IdentityUpdate(new_identity) => {
                if let Some(NewConnectionState::EnterPassword { identity, .. }) =
                    &mut self.new_connection
                {
                    *identity = new_identity;
                }
            }
            Message::NetworkManager(event) => match event {
                network_manager::Event::Init {
                    conn,
                    sender,
                    state,
                } => {
                    self.nm_sender = Some(sender);
                    self.update_nm_state(state);
                    self.conn = Some(conn);
                }
                network_manager::Event::WiFiEnabled(_)
                | network_manager::Event::WirelessAccessPoints
                | network_manager::Event::ActiveConns => {
                    if let Some(conn) = self.conn.clone() {
                        return Task::future(async move {
                            let conn = conn.clone();
                            NetworkManagerState::new(&conn).await
                        })
                        .map(|res| match res {
                            Ok(s) => Message::UpdateState(s),
                            Err(err) => Message::Error(err.to_string()),
                        })
                        .map(cosmic::Action::App);
                    }
                }
                network_manager::Event::RequestResponse {
                    mut state,
                    success,
                    req,
                } => {
                    if let network_manager::Request::SelectAccessPoint(
                        ssid,
                        hw_address,
                        _network_type,
                        secret_tx,
                    ) = &req
                    {
                        let conn_match = self
                            .new_connection
                            .as_ref()
                            .is_some_and(|c| c.ssid() == ssid.as_ref() );

                        if conn_match && success {
                            if let Some(ActiveConnectionInfo::WiFi { state, .. }) = state
                                .active_conns
                                .iter_mut()
                                .find(|ap| {
                                    let ap_hw_address = match ap {
                                        ActiveConnectionInfo::Wired { hw_address, .. }
                                        | ActiveConnectionInfo::WiFi { hw_address, .. } => {
                                            HwAddress::from_str(&hw_address).unwrap()
                                        }
                                        ActiveConnectionInfo::Vpn { .. } => HwAddress::default(),
                                    };
                                    ap.name().as_str() == ssid.as_ref()})
                            {
                                *state = ActiveConnectionState::Activated;
                            }
                            self.failed_known_ssids.remove(ssid);
                            self.new_connection = None;
                            self.show_visible_networks = false;
                        } else if !matches!(
                            &self.new_connection,
                            Some(NewConnectionState::EnterPassword { .. })
                        ) && !success {
                            self.failed_known_ssids.insert(ssid.clone());
                        }
                    } else if let network_manager::Request::Authenticate {
                        ssid,
                        identity: _,
                        password: _,
                        secret_tx,
                        interface
                    } = &req
                    {
                        if let Some(NewConnectionState::Waiting(access_point)) =
                            self.new_connection.as_ref()
                        {
                            if !success
                                && ssid.as_str() == access_point.ssid.as_ref()
                            {
                                self.new_connection =
                                    Some(NewConnectionState::Failure(access_point.clone()));
                            } else {
                                self.show_visible_networks = false;
                            }
                        } else if let Some(NewConnectionState::EnterPassword {
                            access_point, ..
                        }) = self.new_connection.as_ref()
                        {
                            if success && ssid.as_str() == access_point.ssid.as_ref() {
                                self.new_connection = None;
                                self.show_visible_networks = false;
                            }
                        }
                    } else if self
                    .new_connection
                    .as_ref()
                    .map(NewConnectionState::ssid).is_some_and(|ssid| {
                        state.active_conns.iter().any(|c|
                            matches!(c, ActiveConnectionInfo::WiFi { name, state: ActiveConnectionState::Activated, .. } if ssid == name)
                        )
                    }) {
                        self.new_connection = None;
                        self.show_visible_networks = false;
                    }

                    if !matches!(req, network_manager::Request::Reload)
                        && matches!(state.connectivity, NmConnectivityState::Portal)
                    {
                        let mut browser = std::process::Command::new("xdg-open");
                        browser.arg("http://204.pop-os.org/");

                        tokio::spawn(cosmic::process::spawn(browser));
                    }

                    self.update_nm_state(state);
                }

                cosmic_settings_network_manager_subscription::Event::Devices => {
                    if let Some(conn) = self.conn.clone() {
                        return update_devices(conn).map(cosmic::Action::App);
                    }
                }
                cosmic_settings_network_manager_subscription::Event::WiFiCredentials {
                    ssid,
                    password,
                    security_type,
                } => {}
            },
            Message::NetworkManagerConnect(connection) => {
                return cosmic::task::batch(vec![
                    self.connect(connection.clone()),
                    connection_settings(connection),
                ]);
            }
            Message::PasswordUpdate(entered_pw) => {
                if let Some(NewConnectionState::EnterPassword { password, .. }) =
                    &mut self.new_connection
                {
                    *password = entered_pw;
                }
            }
            Message::UpdateState(network_manager_state) => {
                self.update_nm_state(network_manager_state);
            }
            Message::UpdateDevices(device_infos) => {
                self.nm_state.devices = device_infos.into_iter().map(Arc::new).collect();
            }
            Message::WiFiEnable(enable) => {
                if let Some(sender) = self.nm_sender.as_mut() {
                    if let Err(err) =
                        sender.unbounded_send(network_manager::Request::SetWiFi(enable))
                    {
                        if err.is_disconnected() {
                            return system_conn().map(cosmic::Action::App);
                        }

                        tracing::error!("{err:?}");
                    }
                    if let Err(err) = sender.unbounded_send(network_manager::Request::Reload) {
                        if err.is_disconnected() {
                            return system_conn().map(cosmic::Action::App);
                        }

                        tracing::error!("{err:?}");
                    }
                }
            }
            Message::SecretAgent(agent_event) => match agent_event {
                nm_secret_agent::Event::RequestSecret {
                    uuid,
                    name,
                    description,
                    previous,
                    tx,
                    ..
                } => {
                    if let Some(state) = self.new_connection.as_mut() {
                        match state {
                            NewConnectionState::EnterPassword { access_point, .. }
                            | NewConnectionState::Waiting(access_point)
                            | NewConnectionState::Failure(access_point) => {
                                if self
                                    .nm_state
                                    .ssid_to_uuid
                                    .get(access_point.ssid.as_ref())
                                    .is_some_and(|ap_uuid| ap_uuid.as_ref() == uuid.as_str())
                                {
                                    *state = NewConnectionState::EnterPassword {
                                        access_point: access_point.clone(),
                                        description,
                                        identity: String::new(),
                                        password: String::new().into(),
                                        password_hidden: true,
                                    }
                                }
                            }
                        }
                    } else if self.nm_state.known_vpns.contains_key(uuid.as_str()) {
                        self.nm_state.requested_vpn = Some(RequestedVpn {
                            name,
                            uuid: uuid.into(),
                            description,
                            password: previous,
                            password_hidden: true,
                            tx,
                        });
                    }
                }
                nm_secret_agent::Event::CancelGetSecrets { .. } => {
                    self.new_connection = None;
                    self.nm_state.requested_vpn = None;
                }
                nm_secret_agent::Event::Failed(error) => {
                    tracing::error!("Error from secret agent: {error:?}");
                }
            },
            Message::KnownConnections(index_map) => {
                self.nm_state.known_vpns = index_map;
            }
            Message::Refresh => {
                if let Some(conn) = self.conn.clone() {
                    return Task::batch(vec![
                        update_state(conn.clone()),
                        update_devices(conn.clone()),
                        load_vpns(conn),
                    ])
                    .map(cosmic::Action::App);
                }
            }
            Message::ToggleVPNPasswordVisibility => {
                if let Some(requested_vpn) = self.nm_state.requested_vpn.as_mut() {
                    requested_vpn.password_hidden = !requested_vpn.password_hidden;
                }
            }
            Message::ConnectVPNWithPassword => {
                if let Some(RequestedVpn { password, tx, .. }) = self.nm_state.requested_vpn.take()
                {
                    return Task::future(async move {
                        let mut guard = tx.lock().await;
                        if let Some(tx) = guard.take() {
                            let _ = tx.send(password);
                        }
                        Message::Refresh
                    })
                    .map(cosmic::Action::App);
                }
            }
            Message::VPNPasswordUpdate(secure_string) => {
                if let Some(requested_vpn) = self.nm_state.requested_vpn.as_mut() {
                    requested_vpn.password = secure_string;
                }
            }
            Message::CancelVPNConnection => {
                self.nm_state.requested_vpn = None;
            }
        }
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

        let mut vpn_ethernet_col = column![];
        let mut known_wifi = Vec::new();
        for conn in &self.nm_state.nm_state.active_conns {
            match conn {
                ActiveConnectionInfo::Vpn { name, ip_addresses } => {
                    if self.active_device.as_ref().is_some_and(|d| {
                        d.active_connection.as_ref().is_none_or(|a| a.0.id != *name)
                    }) {
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
                    if self.active_device.as_ref().is_some_and(|d| {
                        d.active_connection.as_ref().is_none_or(|a| a.0.id != *name)
                    }) {
                        continue;
                    }
                    let mut ipv4 = Vec::with_capacity(ip_addresses.len() + 1);
                    ipv4.push(text::body(name).into());
                    for addr in ip_addresses {
                        ipv4.push(text(format!("{}: {}", fl!("ipv4"), addr)).size(12).into());
                    }

                    let mut right_column = vec![text::body(fl!("connected")).into()];

                    // Only show speed if it's greater than 0
                    if *speed > 0 {
                        let speed_text = if *speed >= 1_000_000 {
                            let tbps = *speed as f64 / 1_000_000.0;
                            if tbps.fract() == 0.0 {
                                format!("{} {}", tbps as u32, fl!("terabits-per-second"))
                            } else {
                                format!("{:.1} {}", tbps, fl!("terabits-per-second"))
                            }
                        } else if *speed >= 1_000 {
                            let gbps = *speed as f64 / 1_000.0;
                            if gbps.fract() == 0.0 {
                                format!("{} {}", gbps as u32, fl!("gigabits-per-second"))
                            } else {
                                format!("{:.1} {}", gbps, fl!("gigabits-per-second"))
                            }
                        } else {
                            format!("{speed} {}", fl!("megabits-per-second"))
                        };
                        right_column.push(text(speed_text).size(12).into());
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
                            Column::with_children(right_column)
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
                    if self.active_device.as_ref().is_some_and(|d| {
                        d.active_connection.as_ref().is_none_or(|a| a.0.id != *name)
                    }) {
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
                    }
                    if self.failed_known_ssids.contains(name.as_str()) {
                        btn_content.push(
                            cosmic::widget::button::icon(
                                from_name("view-refresh-symbolic").size(16),
                            )
                            .icon_size(16)
                            .on_press(Message::ResetFailedKnownSsid(
                                name.clone(),
                                HwAddress::from_str(&hw_address).unwrap(),
                            ))
                            .into(),
                        );
                    }

                    known_wifi.push(Element::from(
                        column![
                            menu_button(
                                Row::with_children(btn_content)
                                    .align_y(Alignment::Center)
                                    .spacing(8)
                            )
                            .on_press(Message::Disconnect(
                                Arc::from(name.as_str()),
                                HwAddress::from_str(&hw_address).unwrap()
                            ))
                        ]
                        .align_x(Alignment::Center),
                    ));
                }
            }
        }

        let mut content = if let Some(active_device) = self.active_device.as_ref() {
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
                    text::body(&active_device.interface)
                        .width(Length::Fill)
                        .height(Length::Fixed(24.0))
                        .align_y(Alignment::Center),
                ])
                .on_press(Message::SelectDevice(None))
            ]
        } else {
            column![
                // TODO: remove excesive column!
                Element::from(
                    column![
                        vpn_ethernet_col,
                        padded_control(
                            anim!(
                                //toggler
                                AIRPLANE_MODE,
                                &self.timeline,
                                fl!("airplane-mode"),
                                self.nm_state.nm_state.airplane_mode,
                                |_chain, enable| { Message::ToggleAirplaneMode(enable) },
                            )
                            .text_size(14)
                            .width(Length::Fill)
                        ),
                        padded_control(divider::horizontal::default())
                            .padding([space_xxs, space_s]),
                    ]
                    .align_x(Alignment::Center)
                ),
                padded_control(
                    anim!(
                        //toggler
                        WIFI,
                        &self.timeline,
                        fl!("wifi"),
                        self.nm_state.nm_state.wifi_enabled,
                        |_chain, enable| { Message::WiFiEnable(enable) },
                    )
                    .text_size(14)
                    .width(Length::Fill)
                ),
            ]
            .align_x(Alignment::Center)
        };
        if self.nm_state.nm_state.airplane_mode {
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

            // Show VPN connections even in airplane mode
            if !self.nm_state.known_vpns.is_empty() {
                content = content.push(vpn_section(
                    &self.nm_state,
                    self.show_available_vpns,
                    space_xxs,
                    space_s,
                ));
            }

            return self.view_window_return(content);
        }

        if !self.nm_state.nm_state.wifi_enabled && !self.nm_state.known_vpns.is_empty() {
            // Add VPN connections section when WiFi is disabled
            content = content.push(vpn_section(
                &self.nm_state,
                self.show_available_vpns,
                space_xxs,
                space_s,
            ));

            return self.view_window_return(content);
        }

        content = content
            .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));

        // TODO sorting?
        let wireless_hw_devices = self
            .nm_state
            .devices
            .iter()
            .filter(|d| matches!(d.device_type, network_manager::devices::DeviceType::Wifi))
            .collect::<Vec<_>>();

        if wireless_hw_devices.len() > 1 && self.active_device.is_none() {
            for interface in wireless_hw_devices {
                let display_name = interface.interface.to_string();

                let is_connected = interface.active_connection.is_some();
                let mut btn_content = vec![
                    column![
                        text::body(display_name),
                        Column::with_children([text("Adapter").size(10).into()])
                    ]
                    .width(Length::Fill)
                    .into(),
                ];
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
                content = content.push(Element::from(
                    menu_button(
                        Row::with_children(btn_content)
                            .align_y(Alignment::Center)
                            .spacing(8),
                    )
                    .on_press(Message::SelectDevice(Some(interface.clone()))),
                ));
            }

            return self.view_window_return(content);
        }

        for known in &self.nm_state.nm_state.known_access_points {
            if let Some(active_device) = self.active_device.as_ref() {
                if active_device
                    .known_connections
                    .iter()
                    .all(|c| &c.id != known.ssid.as_ref())
                {
                    continue;
                }
            }
            let mut btn_content = Vec::with_capacity(2);
            let ssid = text::body(known.ssid.as_ref()).width(Length::Fill);
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

            if self.failed_known_ssids.contains(known.ssid.as_ref()) {
                btn_content.push(
                    cosmic::widget::button::icon(from_name("view-refresh-symbolic").size(16))
                        .icon_size(16)
                        .on_press(Message::ResetFailedKnownSsid(
                            known.ssid.to_string(),
                            known.hw_address,
                        ))
                        .into(),
                );
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
                | DeviceState::NeedAuth => {
                    btn.on_press(Message::Connect(known.ssid.clone(), known.hw_address))
                }
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
            if !self.nm_state.known_vpns.is_empty() {
                content = content.push(vpn_section(
                    &self.nm_state,
                    self.show_available_vpns,
                    space_xxs,
                    space_s,
                ));
            }
            return self.view_window_return(content);
        }

        if let Some(new_conn_state) = self.new_connection.as_ref() {
            match new_conn_state {
                NewConnectionState::EnterPassword {
                    access_point,
                    description,
                    identity,
                    password,
                    password_hidden,
                } => {
                    let id = padded_control(
                        row![
                            icon::from_name("network-wireless-acquiring-symbolic")
                                .size(24)
                                .symbolic(true),
                            text::body(access_point.ssid.as_ref()),
                        ]
                        .align_y(Alignment::Center)
                        .spacing(12),
                    );
                    content = content.push(id);

                    let is_enterprise = matches!(access_point.network_type, NetworkType::EAP);
                    let enter_password_col =
                        column![]
                            .push_maybe(is_enterprise.then(|| text::body(fl!("identity"))))
                            .push_maybe(is_enterprise.then(|| {
                                text_input::text_input("", identity)
                                    .on_input(|i| Message::IdentityUpdate(i))
                            }))
                            .push(text::body(fl!("enter-password")))
                            .push_maybe(description.as_ref().map(|d| text::body(d.clone())))
                            .push(
                                text_input::secure_input(
                                    "",
                                    password.unsecure(),
                                    Some(Message::TogglePasswordVisibility),
                                    *password_hidden,
                                )
                                .on_input(|s| Message::PasswordUpdate(SecureString::from(s)))
                                .on_paste(|s| Message::PasswordUpdate(SecureString::from(s)))
                                .on_submit(|_| Message::ConnectWithPassword),
                            )
                            .push_maybe(access_point.wps_push.then(|| {
                                container(text::body(fl!("router-wps-button"))).padding(8)
                            }))
                            .push(
                                row![
                                    button::standard(fl!("cancel"))
                                        .on_press(Message::CancelNewConnection),
                                    button::suggested(fl!("connect"))
                                        .on_press(Message::ConnectWithPassword)
                                ]
                                .spacing(24),
                            );
                    let col =
                        padded_control(enter_password_col.spacing(8).align_x(Alignment::Center))
                            .align_x(Alignment::Center);
                    content = content.push(col);
                }
                NewConnectionState::Waiting(access_point) => {
                    let id = row![
                        icon::from_name("network-wireless-acquiring-symbolic")
                            .size(24)
                            .symbolic(true),
                        text::body(access_point.ssid.as_ref()),
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
                            text::body(access_point.ssid.as_ref()),
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
            let mut list_col =
                Vec::with_capacity(self.nm_state.nm_state.wireless_access_points.len());
            for ap in &self.nm_state.nm_state.wireless_access_points {
                if self.nm_state.nm_state.active_conns.iter().any(|a| {
                    let hw_address = active_conn_hw_address(a);
                    ap.ssid.as_ref() == &a.name() && ap.hw_address == hw_address
                }) {
                    continue;
                }
                let button = menu_button(
                    row![
                        icon::from_name(wifi_icon(ap.strength))
                            .size(16)
                            .symbolic(true),
                        text::body(ap.ssid.as_ref()).align_y(Alignment::Center)
                    ]
                    .align_y(Alignment::Center)
                    .spacing(12),
                )
                .on_press(Message::SelectWirelessAccessPoint(ap.clone()));
                list_col.push(button.into());
            }
            content = content
                .push(scrollable(Column::with_children(list_col)).height(Length::Fixed(300.0)));
        }

        // Add VPN connections section after wireless networks when they are expanded
        if !self.nm_state.known_vpns.is_empty() && self.nm_state.nm_state.wifi_enabled {
            content = content.push(vpn_section(
                &self.nm_state,
                self.show_available_vpns,
                space_xxs,
                space_s,
            ));
        }

        self.view_window_return(content)
    }

    fn subscription(&self) -> Subscription<Message> {
        let timeline = self
            .timeline
            .as_subscription()
            .map(|(_, now)| Message::Frame(now));
        let token_sub = activation_token_subscription(0).map(Message::Token);

        Subscription::batch([timeline, token_sub])
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}

fn active_conn_hw_address(conn: &ActiveConnectionInfo) -> HwAddress {
    match conn {
        ActiveConnectionInfo::Wired { hw_address, .. }
        | ActiveConnectionInfo::WiFi { hw_address, .. } => HwAddress::from_str(hw_address).unwrap(),
        ActiveConnectionInfo::Vpn { .. } => HwAddress::default(),
    }
}
