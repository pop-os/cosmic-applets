use indexmap::IndexMap;
use nmrs::{
    ActiveConnection, ActiveConnectionState, ConnectType, ConnectivityState, EapOptions,
    NetworkEvent, NetworkManager as NmrsManager, NetworkSnapshot, WifiSecurity,
    agent::{SecretAgent, SecretAgentCapabilities, SecretRequest, SecretResponder, SecretSetting},
};
use rustc_hash::FxHashSet;
use secure_string::SecureString;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    fmt,
    str::FromStr,
    sync::{Arc, LazyLock},
};

use cosmic::{
    Element, Task, app,
    applet::{
        menu_button, menu_control_padding, padded_control,
        token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    },
    cctk::sctk::reexports::calloop,
    cosmic_theme::Spacing,
    iced::core::window,
    iced::{
        Alignment, Length, Subscription,
        platform_specific::shell::wayland::commands::popup::destroy_popup,
    },
    surface, theme,
    widget::{
        Id, button, column, container, divider,
        icon::{self, from_name},
        indeterminate_circular, row, scrollable, secure_input, text, text_input, toggler,
    },
};

use futures::{StreamExt, lock::Mutex as AsyncMutex};

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

impl From<NewConnectionState> for AccessPoint {
    fn from(connection_state: NewConnectionState) -> Self {
        match connection_state {
            NewConnectionState::EnterPassword { access_point, .. } => access_point,
            NewConnectionState::Waiting(access_point) => access_point,
            NewConnectionState::Failure(access_point) => access_point,
        }
    }
}

pub static SECURE_INPUT_WIFI: LazyLock<Id> = LazyLock::new(Id::unique);

type Uuid = Arc<str>;
type Ssid = Arc<str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkType {
    Open,
    Password,
    Eap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    Unknown,
    Unmanaged,
    Unavailable,
    Disconnected,
    NeedAuth,
    Activated,
    Failed,
    Other,
}

impl From<&nmrs::DeviceState> for DeviceState {
    fn from(state: &nmrs::DeviceState) -> Self {
        match state {
            nmrs::DeviceState::Unmanaged => Self::Unmanaged,
            nmrs::DeviceState::Unavailable => Self::Unavailable,
            nmrs::DeviceState::Disconnected => Self::Disconnected,
            nmrs::DeviceState::NeedAuth => Self::NeedAuth,
            nmrs::DeviceState::Activated => Self::Activated,
            nmrs::DeviceState::Failed => Self::Failed,
            nmrs::DeviceState::Other(_) => Self::Unknown,
            _ => Self::Other,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HwAddress([u8; 6]);

impl HwAddress {
    fn as_string(self) -> String {
        self.0
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }
}

impl FromStr for HwAddress {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0; 6];
        let mut parts = value.split(':');
        for byte in &mut bytes {
            let Some(part) = parts.next() else {
                return Err(());
            };
            *byte = u8::from_str_radix(part, 16).map_err(|_| ())?;
        }
        if parts.next().is_some() {
            return Err(());
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for HwAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_string())
    }
}

fn same_access_point(left: &AccessPoint, right: &AccessPoint) -> bool {
    left.ssid == right.ssid
        && left.hw_address == right.hw_address
        && left.interface == right.interface
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
    pub ssid: Ssid,
    pub network_type: NetworkType,
    pub hw_address: HwAddress,
    pub strength: u8,
    pub state: DeviceState,
    pub working: bool,
    pub wps_push: bool,
    pub interface: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ActiveConnectionInfo {
    Wired {
        name: String,
        hw_address: String,
        speed: u32,
        ip4_address: Option<String>,
        ip6_address: Option<String>,
    },
    WiFi {
        name: String,
        ip4_address: Option<String>,
        ip6_address: Option<String>,
        state: ActiveConnectionState,
        strength: u8,
        hw_address: String,
    },
    Vpn {
        name: String,
        ip4_address: Option<String>,
        ip6_address: Option<String>,
    },
}

impl ActiveConnectionInfo {
    fn name(&self) -> &str {
        match self {
            Self::Wired { name, .. } | Self::WiFi { name, .. } | Self::Vpn { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkManagerState {
    pub wifi_enabled: bool,
    pub airplane_mode: bool,
    pub connectivity: ConnectivityState,
    pub active_conns: Vec<ActiveConnectionInfo>,
    pub known_access_points: Vec<AccessPoint>,
    pub wireless_access_points: Vec<AccessPoint>,
}

impl Default for NetworkManagerState {
    fn default() -> Self {
        Self {
            wifi_enabled: true,
            airplane_mode: false,
            connectivity: ConnectivityState::Unknown,
            active_conns: Vec::new(),
            known_access_points: Vec::new(),
            wireless_access_points: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceType {
    Wifi,
}

#[derive(Clone, Debug)]
pub struct DeviceConnection {
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub interface: String,
    pub device_type: DeviceType,
    pub active_connection: Option<(DeviceConnection,)>,
    pub known_connections: Vec<DeviceConnection>,
}

#[derive(Debug, Clone)]
pub struct AppletSnapshot {
    pub state: NetworkManagerState,
    pub devices: Vec<DeviceInfo>,
    pub known_vpns: IndexMap<Uuid, ConnectionSettings>,
    pub ssid_to_uuid: BTreeMap<Box<str>, Box<str>>,
    pub captive_portal_url: Option<String>,
}

#[derive(Default, Debug, Clone)]
pub struct MyNetworkState {
    pub known_vpns: IndexMap<Uuid, ConnectionSettings>,
    pub ssid_to_uuid: BTreeMap<Box<str>, Box<str>>,
    pub devices: Vec<Arc<DeviceInfo>>,
    pub nm_state: NetworkManagerState,
    pub requested_vpn: Option<RequestedVpn>,
}

/// Shared, take-once handle to an `nmrs` [`SecretResponder`]. Cloned freely
/// across `Message` boundaries; the first consumer to `lock().take()` it owns
/// the reply to NetworkManager.
pub type SecretResponderHandle = Arc<AsyncMutex<Option<SecretResponder>>>;

#[derive(Debug, Clone)]
pub struct RequestedVpn {
    uuid: Arc<str>,
    description: Option<String>,
    password: SecureString,
    password_hidden: bool,
    responder: SecretResponderHandle,
    /// VPN secret keys NM hinted as needed (e.g. `["password"]`). When empty,
    /// `"password"` is used as a fallback.
    secret_keys: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum ConnectionSettings {
    Vpn { id: String },
    Wireguard { id: String },
}

/// Local mirror of the secret-agent events the applet cares about. Sourced
/// from `nmrs::agent` instead of the previous `nm_secret_agent` subscription.
#[derive(Debug, Clone)]
pub enum NmAgentEvent {
    RequestSecret {
        connection_uuid: String,
        connection_id: String,
        setting: AgentSetting,
        responder: SecretResponderHandle,
    },
    CancelGetSecrets,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum AgentSetting {
    WifiPsk { ssid: String },
    WifiEap,
    Vpn { secret_keys: Vec<String> },
    Other,
}

#[derive(Default)]
struct CosmicNetworkApplet {
    core: cosmic::app::Core,
    icon_name: String,
    popup: Option<window::Id>,

    // NM state
    nm_state: MyNetworkState,

    // UI state
    show_visible_networks: bool,
    show_available_vpns: bool,
    new_connection: Option<NewConnectionState>,
    toggle_wifi_ctr: u128,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    failed_known_ssids: FxHashSet<Arc<str>>,

    /// When defined, displays connections for the specific device.
    active_device: Option<Arc<DeviceInfo>>,
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
) -> cosmic::iced::widget::Column<'a, Message, cosmic::Theme> {
    let mut vpn_col = cosmic::widget::column::with_capacity::<'_, Message, cosmic::Theme, _>(4);

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
                row::with_children([
                    button::standard(fl!("cancel"))
                        .on_press(Message::CancelVPNConnection)
                        .into(),
                    button::suggested(fl!("connect"))
                        .on_press(Message::ConnectVPNWithPassword)
                        .into(),
                ])
                .spacing(24)
                .into(),
            ];
            let col: Element<'a, Message> = Element::from(
                padded_control(
                    column::with_children(column_content)
                        .spacing(8)
                        .align_x(Alignment::Center),
                )
                .align_x(Alignment::Center),
            );
            vpn_col = vpn_col.push(col);
        }

        let vpn_toggle_btn = menu_button(row::with_children([
            Element::from(
                text::body(fl!("vpn-connections"))
                    .width(Length::Fill)
                    .height(Length::Fixed(24.0))
                    .align_y(Alignment::Center),
            ),
            container(icon::from_name(dropdown_icon).size(16).symbolic(true))
                .center(Length::Fixed(24.0))
                .into(),
        ]))
        .on_press(Message::ToggleVpnList);

        vpn_col = vpn_col.push(vpn_toggle_btn);

        if show_available_vpns {
            for (uuid, connection) in &nm_state.known_vpns {
                let id = match connection {
                    ConnectionSettings::Vpn { id } | ConnectionSettings::Wireguard { id } => {
                        id.as_str()
                    }
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
                    row::with_children(btn_content)
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

fn ip_address_elements<'a>(
    ip4_address: &Option<String>,
    _ip6_address: &Option<String>,
) -> Vec<Element<'a, Message>> {
    let mut elements = Vec::with_capacity(1);
    if let Some(addr) = ip4_address {
        // Strip the subnet mask (e.g. "10.0.0.2/24" -> "10.0.0.2"); the drop-down
        // only needs the address itself for glancing at the host on a local network.
        let addr = addr.split('/').next().unwrap_or(addr.as_str());
        elements.push(text(format!("{}: {}", fl!("ipv4"), addr)).size(12).into());
    }
    elements
}

fn network_type(security: nmrs::SecurityFeatures) -> NetworkType {
    match security.preferred_connect_type() {
        ConnectType::Open | ConnectType::Owe => NetworkType::Open,
        ConnectType::Eap => NetworkType::Eap,
        ConnectType::Psk | ConnectType::Sae => NetworkType::Password,
        _ => NetworkType::Password,
    }
}

fn wifi_security(
    access_point: &AccessPoint,
    identity: Option<String>,
    password: Option<SecureString>,
) -> WifiSecurity {
    match access_point.network_type {
        NetworkType::Open => WifiSecurity::Open,
        NetworkType::Eap => WifiSecurity::WpaEap {
            opts: EapOptions::new(
                identity.unwrap_or_default(),
                password
                    .as_ref()
                    .map(|password| password.unsecure().to_owned())
                    .unwrap_or_default(),
            ),
        },
        NetworkType::Password => WifiSecurity::WpaPsk {
            psk: password
                .as_ref()
                .map(|password| password.unsecure().to_owned())
                .unwrap_or_default(),
        },
    }
}

fn connect_access_point_task(
    access_point: AccessPoint,
    identity: Option<String>,
    password: Option<SecureString>,
) -> Task<cosmic::Action<Message>> {
    cosmic::task::future(async move {
        let ssid = access_point.ssid.to_string();
        let interface = access_point.interface.clone();
        let bssid = access_point.hw_address.as_string();
        let security = wifi_security(&access_point, identity, password);
        match NmrsManager::new().await {
            Ok(nm) => {
                match nm
                    .connect_to_bssid(&ssid, Some(&bssid), interface.as_deref(), security)
                    .await
                {
                    Ok(()) => Message::ConnectionAttemptFinished {
                        access_point,
                        success: true,
                        error: None,
                    },
                    Err(e) => Message::ConnectionAttemptFinished {
                        access_point,
                        success: false,
                        error: Some(e.to_string()),
                    },
                }
            }
            Err(e) => Message::Error(format!("nmrs init: {e}")),
        }
    })
    .map(cosmic::Action::App)
}

fn snapshot_task() -> Task<Message> {
    cosmic::task::future(async move {
        let nm = match NmrsManager::new().await {
            Ok(nm) => nm,
            Err(e) => return Message::Error(format!("nmrs init: {e}")),
        };

        match nm.snapshot().await {
            Ok(snapshot) => Message::Snapshot(snapshot_to_applet(snapshot)),
            Err(e) => Message::Error(format!("snapshot: {e}")),
        }
    })
}

fn network_events_task() -> Task<Message> {
    cosmic::Task::stream(async_fn_stream::fn_stream(|emitter| async move {
        let nm = match NmrsManager::new().await {
            Ok(nm) => nm,
            Err(e) => {
                let _ = emitter
                    .emit(Message::Error(format!("nmrs init: {e}")))
                    .await;
                return;
            }
        };
        let mut events = match nm.network_events().await {
            Ok(events) => events,
            Err(e) => {
                let _ = emitter
                    .emit(Message::Error(format!("network events: {e}")))
                    .await;
                return;
            }
        };

        while let Some(event) = events.next().await {
            match event {
                Ok(event) => {
                    let _ = emitter.emit(Message::NetworkEvent(event)).await;
                }
                Err(e) => {
                    let _ = emitter
                        .emit(Message::Error(format!("network event: {e}")))
                        .await;
                }
            }
        }
    }))
}

fn snapshot_to_applet(snapshot: NetworkSnapshot) -> AppletSnapshot {
    let summary = snapshot.applet_summary();
    let mut known_vpns = IndexMap::new();
    for vpn in summary.saved_vpns.values() {
        let uuid: Uuid = Arc::from(vpn.uuid.as_str());
        let entry = match vpn.kind {
            Some(nmrs::VpnKind::WireGuard) => ConnectionSettings::Wireguard { id: vpn.id.clone() },
            _ => ConnectionSettings::Vpn { id: vpn.id.clone() },
        };
        known_vpns.insert(uuid, entry);
    }

    let mut ssid_to_uuid = BTreeMap::new();
    for (ssid, profiles) in &summary.known_wifi {
        if let Some(profile) = profiles.first() {
            ssid_to_uuid.insert(
                ssid.clone().into_boxed_str(),
                profile.uuid.clone().into_boxed_str(),
            );
        }
    }

    let active_conns = snapshot
        .active_connections
        .iter()
        .filter_map(|conn| match conn {
            ActiveConnection::Wired(wired) => Some(ActiveConnectionInfo::Wired {
                name: wired.id.clone(),
                hw_address: wired.hw_address.clone().unwrap_or_default(),
                speed: wired.speed_mbps.unwrap_or_default(),
                ip4_address: wired.ip4_address.clone(),
                ip6_address: wired.ip6_address.clone(),
            }),
            ActiveConnection::Wifi(wifi) => Some(ActiveConnectionInfo::WiFi {
                name: wifi.ssid.clone(),
                ip4_address: wifi.ip4_address.clone(),
                ip6_address: wifi.ip6_address.clone(),
                state: wifi.state,
                strength: wifi.strength.unwrap_or_default(),
                hw_address: wifi.bssid.clone().unwrap_or_default(),
            }),
            ActiveConnection::Vpn(vpn) => Some(ActiveConnectionInfo::Vpn {
                name: vpn.id.clone(),
                ip4_address: vpn.ip4_address.clone(),
                ip6_address: vpn.ip6_address.clone(),
            }),
            ActiveConnection::Other(_) | _ => None,
        })
        .collect();

    let mut wireless_access_points = summary
        .wifi_groups
        .iter()
        .filter(|group| !group.ssid.is_empty() && !group.strongest.ssid_bytes.is_empty())
        .map(|group| {
            let strongest = &group.strongest;
            AccessPoint {
                ssid: Arc::from(group.ssid.as_str()),
                network_type: network_type(strongest.security),
                hw_address: HwAddress::from_str(&strongest.bssid).unwrap_or_default(),
                strength: strongest.strength,
                state: DeviceState::from(&strongest.device_state),
                working: false,
                wps_push: strongest.security.wps,
                interface: Some(group.interface.clone()),
            }
        })
        .collect::<Vec<_>>();
    wireless_access_points.sort_by(|a, b| b.strength.cmp(&a.strength));

    let known_access_points = summary
        .wifi_groups
        .iter()
        .filter(|group| group.known || group.active)
        .map(|group| {
            let strongest = &group.strongest;
            let device_state = DeviceState::from(&strongest.device_state);
            AccessPoint {
                ssid: Arc::from(group.ssid.as_str()),
                network_type: network_type(strongest.security),
                hw_address: HwAddress::from_str(&strongest.bssid).unwrap_or_default(),
                strength: strongest.strength,
                state: if group.active {
                    DeviceState::Activated
                } else if matches!(device_state, DeviceState::Activated) {
                    DeviceState::Disconnected
                } else {
                    device_state
                },
                working: false,
                wps_push: strongest.security.wps,
                interface: Some(group.interface.clone()),
            }
        })
        .collect::<Vec<_>>();

    let devices = snapshot
        .wifi_devices
        .iter()
        .map(|device| {
            let known_connections = summary
                .known_wifi
                .iter()
                .filter(|(ssid, _)| {
                    summary
                        .wifi_groups
                        .iter()
                        .any(|group| group.interface == device.interface && group.ssid == **ssid)
                })
                .map(|(ssid, _)| DeviceConnection { id: ssid.clone() })
                .collect();
            DeviceInfo {
                interface: device.interface.clone(),
                device_type: DeviceType::Wifi,
                active_connection: device
                    .active_ssid
                    .as_ref()
                    .map(|ssid| (DeviceConnection { id: ssid.clone() },)),
                known_connections,
            }
        })
        .collect();

    AppletSnapshot {
        state: NetworkManagerState {
            wifi_enabled: snapshot.wifi.enabled,
            airplane_mode: summary.airplane_mode.is_airplane_mode(),
            connectivity: summary.connectivity.state,
            active_conns,
            known_access_points,
            wireless_access_points,
        },
        devices,
        known_vpns,
        ssid_to_uuid,
        captive_portal_url: summary.connectivity.captive_portal_url.clone(),
    }
}

impl CosmicNetworkApplet {
    fn apply_snapshot(&mut self, snapshot: AppletSnapshot) {
        let previous_connectivity = self.nm_state.nm_state.connectivity;
        self.update_nm_state(snapshot.state);
        self.nm_state.devices = snapshot.devices.into_iter().map(Arc::new).collect();
        self.nm_state.known_vpns = snapshot.known_vpns;
        self.nm_state.ssid_to_uuid = snapshot.ssid_to_uuid;

        if !previous_connectivity.is_captive() && self.nm_state.nm_state.connectivity.is_captive() {
            let mut browser = std::process::Command::new("xdg-open");
            browser.arg(
                snapshot
                    .captive_portal_url
                    .as_deref()
                    .unwrap_or("http://204.pop-os.org/"),
            );
            tokio::spawn(cosmic::process::spawn(browser));
        }
    }

    fn update_nm_state(&mut self, mut new_state: NetworkManagerState) {
        self.update_togglers(&new_state);
        // check for failed conns that can be reset
        for new_s in &mut new_state.active_conns {
            let ActiveConnectionInfo::WiFi { state, .. } = new_s else {
                continue;
            };

            if matches!(state, ActiveConnectionState::Activated) {
                self.failed_known_ssids.remove(new_s.name());
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
        if self.nm_state.nm_state.wifi_enabled != state.wifi_enabled {
            self.nm_state.nm_state.wifi_enabled = state.wifi_enabled;
        }

        if self.nm_state.nm_state.airplane_mode != state.airplane_mode {
            self.nm_state.nm_state.airplane_mode = state.airplane_mode;
        }
    }

    fn view_window_return<'a>(
        &self,
        mut content: cosmic::iced::widget::Column<'a, Message, cosmic::Theme>,
    ) -> Element<'a, Message> {
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
        cosmic::task::future(async move {
            match NmrsManager::new().await {
                Ok(nm) => match nm.connect_vpn_by_uuid(&uuid).await {
                    Ok(()) => Message::Refresh,
                    Err(e) => Message::Error(format!("activate VPN {uuid}: {e}")),
                },
                Err(e) => Message::Error(format!("nmrs init: {e}")),
            }
        })
        .map(cosmic::Action::App)
    }
}

/// Registers an `nmrs` secret agent on the system bus and yields its
/// requests + cancellations as [`NmAgentEvent`] for the applet to handle.
fn secret_agent_task(identifier: String) -> Task<NmAgentEvent> {
    cosmic::Task::stream(async_fn_stream::fn_stream(move |emitter| async move {
        let registration = SecretAgent::builder()
            .with_identifier(identifier)
            .with_object_path("/org/freedesktop/NetworkManager/SecretAgent")
            .with_capabilities(SecretAgentCapabilities::VPN_HINTS)
            .register()
            .await;

        let (mut handle, mut requests) = match registration {
            Ok(pair) => pair,
            Err(e) => {
                let _ = emitter.emit(NmAgentEvent::Failed(e.to_string())).await;
                return;
            }
        };

        loop {
            tokio::select! {
                req = requests.next() => match req {
                    Some(req) => {
                        let event = secret_request_to_event(req);
                        let _ = emitter.emit(event).await;
                    }
                    None => break,
                },
                cancel = handle.cancellations().next() => match cancel {
                    Some(_reason) => {
                        let _ = emitter.emit(NmAgentEvent::CancelGetSecrets).await;
                    }
                    None => break,
                },
            }
        }

        if let Err(e) = handle.unregister().await {
            tracing::warn!("failed to unregister secret agent: {e}");
        }
    }))
}

fn secret_request_to_event(req: SecretRequest) -> NmAgentEvent {
    let setting = match req.setting {
        SecretSetting::WifiPsk { ssid } => AgentSetting::WifiPsk { ssid },
        SecretSetting::WifiEap { .. } => AgentSetting::WifiEap,
        SecretSetting::Vpn { .. } => AgentSetting::Vpn {
            secret_keys: req.hints.clone(),
        },
        _ => AgentSetting::Other,
    };

    NmAgentEvent::RequestSecret {
        connection_uuid: req.connection_uuid,
        connection_id: req.connection_id,
        setting,
        responder: Arc::new(AsyncMutex::new(Some(req.responder))),
    }
}

/// Reply with [`NoSecrets`](nmrs::agent::SecretResponder::no_secrets) to free
/// NetworkManager when the applet decides not to use the responder. Dropping
/// it would also auto-reply, but doing it explicitly keeps the log clean.
fn release_responder(responder: SecretResponderHandle) -> Task<cosmic::Action<Message>> {
    cosmic::task::future(async move {
        if let Some(r) = responder.lock().await.take() {
            let _ = r.no_secrets().await;
        }
        Message::NoOp
    })
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    ToggleAirplaneMode(bool),
    ToggleVisibleNetworks,
    SelectWirelessAccessPoint(AccessPoint),
    CancelNewConnection,
    Token(TokenUpdate),
    OpenSettings,
    ResetFailedKnownSsid(String, HwAddress),
    TogglePasswordVisibility,
    FocusSecureInput,
    NoOp,
    #[allow(dead_code)] // required by `cosmic::applet` surface path; not always emitted
    Surface(surface::Action),
    ActivateVpn(Arc<str>),   // UUID of VPN to activate
    DeactivateVpn(Arc<str>), // UUID of VPN to deactivate
    ToggleVpnList,           // Show/hide available VPNs
    /// An update from the secret agent
    SecretAgent(NmAgentEvent),
    /// Connect to a WiFi network access point.
    Connect(Ssid, HwAddress),
    /// Connect with a password
    ConnectWithPassword,
    /// Disconnect from an access point.
    Disconnect(Ssid, HwAddress),
    ConnectionAttemptFinished {
        access_point: AccessPoint,
        success: bool,
        error: Option<String>,
    },
    /// An error occurred.
    Error(String),
    /// Identity update from the dialog
    IdentityUpdate(String),
    /// An update from NetworkManager.
    NetworkEvent(NetworkEvent),
    /// Update the password from the dialog
    PasswordUpdate(SecureString),
    /// Update applet state from NetworkManager.
    Snapshot(AppletSnapshot),
    /// Toggle WiFi access
    WiFiEnable(bool),
    /// Refresh state
    Refresh,
    ToggleVPNPasswordVisibility,
    ConnectVPNWithPassword,
    VPNPasswordUpdate(SecureString),
    CancelVPNConnection,
    /// Selects a device to display connections from
    SelectDevice(Option<Arc<DeviceInfo>>),
}

impl cosmic::Application for CosmicNetworkApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        let applet = Self {
            core,
            icon_name: "network-wired-disconnected-symbolic".to_string(),
            token_tx: None,
            ..Default::default()
        };

        let uuid = uuid::Uuid::new_v4().to_string().replace("-", "_");
        let my_id =
            format!("com.system76.CosmicSettings.Applet._{uuid}.NetworkManager.SecretAgent",);

        (
            applet,
            Task::batch(vec![
                snapshot_task(),
                network_events_task(),
                secret_agent_task(my_id).map(Message::SecretAgent),
            ])
            .map(cosmic::Action::App),
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
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    self.show_visible_networks = false;
                    return destroy_popup(p);
                } else {
                    let mut tasks = Vec::with_capacity(2);
                    tasks.push(snapshot_task().map(cosmic::Action::App));
                    tasks.push(cosmic::surface::surface_task(
                        cosmic::surface::action::app_popup(
                            |_| Default::default(),
                            |app: &mut Self| {
                                let new_id = window::Id::unique();
                                app.popup.replace(new_id);

                                let popup_settings = app.core.applet.get_popup_settings(
                                    app.core.main_window_id().unwrap(),
                                    new_id,
                                    None,
                                    None,
                                    None,
                                );
                                popup_settings
                            },
                            None,
                        ),
                    ));

                    return Task::batch(tasks);
                }
            }
            Message::ToggleAirplaneMode(enabled) => {
                self.toggle_wifi_ctr += 1;
                self.nm_state.nm_state.airplane_mode = enabled;
                return cosmic::task::future(async move {
                    match NmrsManager::new().await {
                        Ok(nm) => match nm.set_airplane_mode(enabled).await {
                            Ok(()) => Message::Refresh,
                            Err(e) => {
                                tracing::warn!("set_airplane_mode partial failure: {e}");
                                Message::Refresh
                            }
                        },
                        Err(e) => Message::Error(format!("nmrs init: {e}")),
                    }
                })
                .map(cosmic::Action::App);
            }
            Message::SelectWirelessAccessPoint(access_point) => {
                if matches!(access_point.network_type, NetworkType::Open) {
                    self.new_connection = Some(NewConnectionState::Waiting(access_point.clone()));
                    return connect_access_point_task(access_point, None, None);
                } else {
                    let known = self
                        .nm_state
                        .nm_state
                        .known_access_points
                        .iter()
                        .any(|known| same_access_point(known, &access_point));
                    if known {
                        self.new_connection =
                            Some(NewConnectionState::Waiting(access_point.clone()));
                        return connect_access_point_task(access_point, None, None);
                    }
                    self.new_connection = Some(NewConnectionState::EnterPassword {
                        access_point,
                        description: None,
                        identity: String::new(),
                        password: String::new().into(),
                        password_hidden: true,
                    });
                    return cosmic::task::message(cosmic::Action::App(Message::FocusSecureInput));
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
            Message::FocusSecureInput => {
                return text_input::focus(SECURE_INPUT_WIFI.clone())
                    .map(|_: ()| cosmic::Action::App(Message::NoOp));
            }
            Message::NoOp => {}
            Message::CancelNewConnection => {
                self.new_connection = None;
            }
            Message::CloseRequested(id) => {
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
                self.show_visible_networks = true;
                let ssid_for_task = ssid.clone();
                let forget_task = cosmic::task::future(async move {
                    match NmrsManager::new().await {
                        Ok(nm) => match nm.forget(&ssid_for_task).await {
                            Ok(()) => Message::Refresh,
                            Err(e) => {
                                tracing::warn!("forget {ssid_for_task} failed: {e}");
                                Message::Refresh
                            }
                        },
                        Err(e) => Message::Error(format!("nmrs init: {e}")),
                    }
                })
                .map(cosmic::Action::App);
                let reconnect_task = connect_access_point_task(ap, None, None);
                return Task::batch(vec![forget_task, reconnect_task]);
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Message::ActivateVpn(uuid) => {
                return self.connect_vpn(uuid.clone());
            }
            Message::DeactivateVpn(uuid) => {
                return cosmic::task::future(async move {
                    match NmrsManager::new().await {
                        Ok(nm) => match nm.disconnect_vpn_by_uuid(&uuid).await {
                            Ok(()) => Message::Refresh,
                            Err(e) => Message::Error(format!("disconnect VPN {uuid}: {e}")),
                        },
                        Err(e) => Message::Error(format!("nmrs init: {e}")),
                    }
                })
                .map(cosmic::Action::App);
            }
            Message::ToggleVpnList => {
                self.show_available_vpns = !self.show_available_vpns;
            }
            Message::Connect(ssid, hw_address) => {
                if let Some(ap) = self
                    .nm_state
                    .nm_state
                    .known_access_points
                    .iter_mut()
                    .find(|c| c.ssid == ssid && c.hw_address == hw_address)
                {
                    ap.working = true;
                    return connect_access_point_task(ap.clone(), None, None);
                }
            }
            Message::ConnectWithPassword => {
                if let Some(NewConnectionState::EnterPassword {
                    password,
                    access_point,
                    identity,
                    ..
                }) = self.new_connection.take()
                {
                    self.new_connection
                        .replace(NewConnectionState::Waiting(access_point.clone()));
                    return connect_access_point_task(access_point, Some(identity), Some(password));
                }
            }
            Message::Disconnect(ssid, hw_address) => {
                self.new_connection = None;
                let interface = self
                    .nm_state
                    .nm_state
                    .known_access_points
                    .iter()
                    .find(|ap| ap.ssid == ssid && ap.hw_address == hw_address)
                    .and_then(|ap| ap.interface.clone());
                if let Some(ActiveConnectionInfo::WiFi { state, .. }) =
                    self.nm_state.nm_state.active_conns.iter_mut().find(|c| {
                        let c_hw_address = match c {
                            ActiveConnectionInfo::Wired { hw_address, .. }
                            | ActiveConnectionInfo::WiFi { hw_address, .. } => {
                                HwAddress::from_str(hw_address).unwrap_or_default()
                            }
                            ActiveConnectionInfo::Vpn { .. } => HwAddress::default(),
                        };
                        c.name() == ssid.as_ref() && c_hw_address == hw_address
                    })
                {
                    *state = ActiveConnectionState::Deactivating;
                }
                return cosmic::task::future(async move {
                    match NmrsManager::new().await {
                        Ok(nm) => match nm.disconnect(interface.as_deref()).await {
                            Ok(()) => Message::Refresh,
                            Err(e) => Message::Error(format!("disconnect {ssid}: {e}")),
                        },
                        Err(e) => Message::Error(format!("nmrs init: {e}")),
                    }
                })
                .map(cosmic::Action::App);
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
            Message::ConnectionAttemptFinished {
                access_point,
                success,
                error,
            } => {
                if success {
                    self.failed_known_ssids.remove(access_point.ssid.as_ref());
                    self.new_connection = None;
                    self.show_visible_networks = false;
                } else {
                    if let Some(error) = error {
                        tracing::warn!("connect {} failed: {error}", access_point.ssid);
                    }
                    self.failed_known_ssids.insert(access_point.ssid.clone());
                    self.new_connection = Some(NewConnectionState::Failure(access_point));
                }
                return snapshot_task().map(cosmic::Action::App);
            }
            Message::NetworkEvent(event) => {
                if matches!(event, NetworkEvent::NetworkManagerRestarted) {
                    tracing::debug!("NetworkManager restarted; refreshing network snapshot");
                }
                return snapshot_task().map(cosmic::Action::App);
            }
            Message::PasswordUpdate(entered_pw) => {
                if let Some(NewConnectionState::EnterPassword { password, .. }) =
                    &mut self.new_connection
                {
                    *password = entered_pw;
                }
            }
            Message::Snapshot(snapshot) => {
                self.apply_snapshot(snapshot);
            }
            Message::WiFiEnable(enable) => {
                self.nm_state.nm_state.wifi_enabled = enable;
                return cosmic::task::future(async move {
                    match NmrsManager::new().await {
                        Ok(nm) => match nm.set_wireless_enabled(enable).await {
                            Ok(()) => Message::Refresh,
                            Err(e) => Message::Error(format!("set_wireless_enabled: {e}")),
                        },
                        Err(e) => Message::Error(format!("nmrs init: {e}")),
                    }
                })
                .map(cosmic::Action::App);
            }
            Message::SecretAgent(agent_event) => match agent_event {
                NmAgentEvent::RequestSecret {
                    connection_uuid,
                    connection_id,
                    setting,
                    responder,
                } => {
                    let description = (!connection_id.is_empty()).then_some(connection_id);
                    let known_vpn = self
                        .nm_state
                        .known_vpns
                        .contains_key(connection_uuid.as_str());

                    let mut consumed = false;

                    if let Some(state) = self.new_connection.as_mut() {
                        match state {
                            NewConnectionState::EnterPassword { access_point, .. }
                            | NewConnectionState::Waiting(access_point)
                            | NewConnectionState::Failure(access_point) => {
                                let matches_ssid = matches!(
                                    &setting,
                                    AgentSetting::WifiPsk { ssid }
                                        if ssid == access_point.ssid.as_ref()
                                );
                                let matches_uuid = self
                                    .nm_state
                                    .ssid_to_uuid
                                    .get(access_point.ssid.as_ref())
                                    .is_some_and(|ap_uuid| {
                                        ap_uuid.as_ref() == connection_uuid.as_str()
                                    });

                                if matches_ssid || matches_uuid {
                                    *state = NewConnectionState::EnterPassword {
                                        access_point: access_point.clone(),
                                        description: description.clone(),
                                        identity: String::new(),
                                        password: String::new().into(),
                                        password_hidden: true,
                                    };
                                }
                            }
                        }
                    } else if known_vpn {
                        let secret_keys = match &setting {
                            AgentSetting::Vpn { secret_keys } => secret_keys.clone(),
                            _ => Vec::new(),
                        };
                        self.nm_state.requested_vpn = Some(RequestedVpn {
                            uuid: connection_uuid.into(),
                            description,
                            password: SecureString::from(String::new()),
                            password_hidden: true,
                            responder: responder.clone(),
                            secret_keys,
                        });
                        consumed = true;
                    }

                    // The applet's Wi-Fi flow re-issues the password through
                    // `Authenticate` rather than the agent. Free NM with
                    // `NoSecrets` so it doesn't sit on a stalled GetSecrets call.
                    if !consumed {
                        return release_responder(responder);
                    }
                }
                NmAgentEvent::CancelGetSecrets => {
                    self.new_connection = None;
                    self.nm_state.requested_vpn = None;
                }
                NmAgentEvent::Failed(error) => {
                    tracing::error!("Error from secret agent: {error}");
                }
            },
            Message::Refresh => {
                return snapshot_task().map(cosmic::Action::App);
            }
            Message::ToggleVPNPasswordVisibility => {
                if let Some(requested_vpn) = self.nm_state.requested_vpn.as_mut() {
                    requested_vpn.password_hidden = !requested_vpn.password_hidden;
                }
            }
            Message::ConnectVPNWithPassword => {
                if let Some(RequestedVpn {
                    password,
                    responder,
                    secret_keys,
                    ..
                }) = self.nm_state.requested_vpn.take()
                {
                    return Task::future(async move {
                        let Some(responder) = responder.lock().await.take() else {
                            return Message::Refresh;
                        };

                        let mut secrets: HashMap<String, String> = HashMap::new();
                        let value = password.unsecure().to_owned();
                        if secret_keys.is_empty() {
                            secrets.insert("password".to_owned(), value);
                        } else {
                            for key in secret_keys {
                                secrets.insert(key, value.clone());
                            }
                        }

                        if let Err(e) = responder.vpn_secrets(secrets).await {
                            tracing::error!("vpn secret reply failed: {e}");
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
                if let Some(req) = self.nm_state.requested_vpn.take() {
                    return Task::future(async move {
                        if let Some(responder) = req.responder.lock().await.take() {
                            let _ = responder.cancel().await;
                        }
                        Message::NoOp
                    })
                    .map(cosmic::Action::App);
                }
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

        let mut vpn_ethernet_col: cosmic::iced::widget::Column<Message, cosmic::Theme> =
            cosmic::widget::column::with_capacity(1);
        let mut known_wifi = Vec::new();
        for conn in &self.nm_state.nm_state.active_conns {
            match conn {
                ActiveConnectionInfo::Vpn {
                    name,
                    ip4_address,
                    ip6_address,
                } => {
                    if self.active_device.as_ref().is_some_and(|d| {
                        d.active_connection.as_ref().is_none_or(|a| a.0.id != *name)
                    }) {
                        continue;
                    }
                    let mut info_col = Vec::with_capacity(3);
                    info_col.push(text::body(name).into());
                    for elem in ip_address_elements(ip4_address, ip6_address) {
                        info_col.push(elem);
                    }
                    vpn_ethernet_col = vpn_ethernet_col.push(
                        column::with_capacity::<Message, cosmic::Theme, _>(2)
                            .push(
                                row::with_children([
                                    Element::from(
                                        icon::icon(
                                            icon::from_name(self.icon_name.clone())
                                                .symbolic(true)
                                                .into(),
                                        )
                                        .size(40),
                                    ),
                                    column::with_children(info_col).into(),
                                    text::body(fl!("connected"))
                                        .width(Length::Fill)
                                        .align_x(Alignment::End)
                                        .into(),
                                ])
                                .align_y(Alignment::Center)
                                .spacing(8)
                                .padding(menu_control_padding()),
                            )
                            .push(
                                padded_control(divider::horizontal::default())
                                    .padding([space_xxs, space_s]),
                            ),
                    );
                }
                ActiveConnectionInfo::Wired {
                    name,
                    hw_address: _,
                    speed,
                    ip4_address,
                    ip6_address,
                } => {
                    if self.active_device.as_ref().is_some_and(|d| {
                        d.active_connection.as_ref().is_none_or(|a| a.0.id != *name)
                    }) {
                        continue;
                    }
                    let mut info_col = Vec::with_capacity(3);
                    info_col.push(text::body(name).into());
                    info_col.extend(ip_address_elements(ip4_address, ip6_address));

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

                    vpn_ethernet_col = vpn_ethernet_col
                        .push(
                            column::with_capacity(2).push(
                                row::with_children([
                                    Element::from(
                                        icon::icon(
                                            icon::from_name(self.icon_name.clone())
                                                .symbolic(true)
                                                .into(),
                                        )
                                        .size(40),
                                    ),
                                    column::with_children(info_col).into(),
                                    column::with_children(right_column)
                                        .width(Length::Fill)
                                        .align_x(Alignment::End)
                                        .into(),
                                ])
                                .align_y(Alignment::Center)
                                .spacing(8)
                                .padding(menu_control_padding()),
                            ),
                        )
                        .push(
                            padded_control(divider::horizontal::default())
                                .padding([space_xxs, space_s]),
                        );
                }
                ActiveConnectionInfo::WiFi {
                    name,
                    ip4_address,
                    ip6_address,
                    state,
                    strength,
                    hw_address,
                } => {
                    if self.active_device.as_ref().is_some_and(|d| {
                        d.active_connection.as_ref().is_none_or(|a| a.0.id != *name)
                    }) {
                        continue;
                    }
                    let ip_elements = ip_address_elements(ip4_address, ip6_address);
                    let mut btn_content = vec![
                        icon::from_name(wifi_icon(*strength))
                            .size(24)
                            .symbolic(true)
                            .into(),
                        column::with_children([
                            text::body(name).into(),
                            column::with_children(ip_elements).into(),
                        ])
                        .width(Length::Fill)
                        .into(),
                    ];
                    match state {
                        ActiveConnectionState::Activating | ActiveConnectionState::Deactivating => {
                            btn_content.push(indeterminate_circular().size(24.0).into());
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
                                HwAddress::from_str(hw_address).unwrap_or_default(),
                            ))
                            .into(),
                        );
                    }

                    known_wifi.push(Element::from(
                        column::with_children([Element::from(
                            menu_button(
                                row::with_children(btn_content)
                                    .align_y(Alignment::Center)
                                    .spacing(8),
                            )
                            .on_press(Message::Disconnect(
                                Arc::from(name.as_str()),
                                HwAddress::from_str(hw_address).unwrap_or_default(),
                            )),
                        )])
                        .align_x(Alignment::Center),
                    ));
                }
            }
        }

        let mut content = cosmic::widget::column::with_capacity(known_wifi.len() + 1);

        if let Some(active_device) = self.active_device.as_ref() {
            let menu_row = row::with_children::<'_, Message, cosmic::Theme, _>([
                Element::<'_, Message>::from(
                    container(
                        icon::from_name("go-previous-symbolic")
                            .size(16)
                            .symbolic(true),
                    )
                    .align_x(Alignment::Start)
                    .align_y(Alignment::Center)
                    .width(Length::Fixed(24.0))
                    .height(Length::Fixed(24.0)),
                ),
                text::body(&active_device.interface)
                    .width(Length::Fill)
                    .height(Length::Fixed(24.0))
                    .align_y(Alignment::Center)
                    .into(),
            ]);
            content = content
                .push(vpn_ethernet_col)
                .push(menu_button(menu_row).on_press(Message::SelectDevice(None)));
        } else {
            let menu_column = column::with_children([
                Element::from(vpn_ethernet_col),
                padded_control(
                    toggler(self.nm_state.nm_state.airplane_mode)
                        .label(fl!("airplane-mode"))
                        .on_toggle(Message::ToggleAirplaneMode)
                        .text_size(14)
                        .width(Length::Fill),
                )
                .into(),
                padded_control(divider::horizontal::default())
                    .padding([space_xxs, space_s])
                    .into(),
            ])
            .align_x(Alignment::Center);
            content = content
                .push(Element::from(menu_column))
                .push(padded_control(
                    toggler(self.nm_state.nm_state.wifi_enabled)
                        .label(fl!("wifi"))
                        .on_toggle(Message::WiFiEnable)
                        .text_size(14)
                        .width(Length::Fill),
                ))
                .align_x(Alignment::Center);
        }
        if self.nm_state.nm_state.airplane_mode {
            content = content.push(
                column::with_children([
                    Element::from(
                        padded_control(divider::horizontal::default())
                            .padding([space_xxs, space_s]),
                    ),
                    icon::from_name("airplane-mode-symbolic")
                        .size(48)
                        .symbolic(true)
                        .into(),
                    text::body(fl!("airplane-mode-on")).into(),
                    text(fl!("turn-off-airplane-mode")).size(12).into(),
                ])
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
            .filter(|d| matches!(d.device_type, DeviceType::Wifi))
            .collect::<Vec<_>>();

        if wireless_hw_devices.len() > 1 && self.active_device.is_none() {
            for interface in wireless_hw_devices {
                let display_name = interface.interface.to_string();

                let is_connected = interface.active_connection.is_some();
                let mut btn_content = vec![
                    column::with_children([
                        text::body(display_name).into(),
                        column::with_children([text("Adapter").size(10).into()]).into(),
                    ])
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
                        row::with_children(btn_content)
                            .align_y(Alignment::Center)
                            .spacing(8),
                    )
                    .on_press(Message::SelectDevice(Some(interface.clone()))),
                ));
            }

            return self.view_window_return(content);
        }

        for known in &self.nm_state.nm_state.known_access_points {
            if matches!(known.state, DeviceState::Activated) {
                continue;
            }
            if let Some(active_device) = self.active_device.as_ref()
                && active_device
                    .known_connections
                    .iter()
                    .all(|c| c.id != *known.ssid)
            {
                continue;
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
                btn_content.push(indeterminate_circular().size(24.0).into());
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
                            known.hw_address.clone(),
                        ))
                        .into(),
                );
            }

            let mut btn = menu_button(
                row::with_children(btn_content)
                    .align_y(Alignment::Center)
                    .spacing(8),
            );
            btn = match known.state {
                // Activated entries are skipped above (shown in the active
                // connections section instead), so no Disconnect arm is needed here.
                DeviceState::Failed
                | DeviceState::Unknown
                | DeviceState::Unmanaged
                | DeviceState::Disconnected
                | DeviceState::NeedAuth => {
                    btn.on_press(Message::Connect(known.ssid.clone(), known.hw_address))
                }
                _ => btn,
            };
            known_wifi.push(Element::from(
                row::with_capacity(1).push(btn).align_y(Alignment::Center),
            ));
        }
        let has_known_wifi = !known_wifi.is_empty();
        content = content.push(column::with_children(known_wifi));
        if has_known_wifi {
            content = content
                .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));
        }

        let dropdown_icon = if self.show_visible_networks {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };
        let available_connections_btn = menu_button(row::with_children([
            Element::from(
                text::body(fl!("visible-wireless-networks"))
                    .width(Length::Fill)
                    .height(Length::Fixed(24.0))
                    .align_y(Alignment::Center),
            ),
            container(icon::from_name(dropdown_icon).size(16).symbolic(true))
                .center(Length::Fixed(24.0))
                .into(),
        ]))
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
                        row::with_children([
                            Element::from(
                                icon::from_name("network-wireless-acquiring-symbolic")
                                    .size(24)
                                    .symbolic(true),
                            ),
                            text::body(access_point.ssid.as_ref()).into(),
                        ])
                        .align_y(Alignment::Center)
                        .spacing(12),
                    );
                    content = content.push(id);

                    let is_enterprise = matches!(access_point.network_type, NetworkType::Eap);
                    let enter_password_col = cosmic::widget::column::with_capacity(4)
                        .push_maybe(is_enterprise.then(|| text::body(fl!("identity"))))
                        .push_maybe(is_enterprise.then(|| {
                            text_input::text_input("", identity).on_input(Message::IdentityUpdate)
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
                            .id(SECURE_INPUT_WIFI.clone())
                            .on_input(|s| Message::PasswordUpdate(SecureString::from(s)))
                            .on_paste(|s| Message::PasswordUpdate(SecureString::from(s)))
                            .on_submit(|_| Message::ConnectWithPassword),
                        )
                        .push_maybe(
                            access_point.wps_push.then(|| {
                                container(text::body(fl!("router-wps-button"))).padding(8)
                            }),
                        )
                        .push(
                            row::with_children([
                                Element::from(
                                    button::standard(fl!("cancel"))
                                        .on_press(Message::CancelNewConnection),
                                ),
                                Element::from(
                                    button::suggested(fl!("connect"))
                                        .on_press(Message::ConnectWithPassword),
                                ),
                            ])
                            .spacing(24),
                        );
                    let col =
                        padded_control(enter_password_col.spacing(8).align_x(Alignment::Center))
                            .align_x(Alignment::Center);
                    content = content.push(col);
                }
                NewConnectionState::Waiting(access_point) => {
                    let id = row::with_children([
                        Element::from(
                            icon::from_name("network-wireless-acquiring-symbolic")
                                .size(24)
                                .symbolic(true),
                        ),
                        text::body(access_point.ssid.as_ref()).into(),
                    ])
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .spacing(12);
                    let connecting = padded_control(
                        row::with_children([
                            Element::from(id),
                            indeterminate_circular().size(24.0).into(),
                        ])
                        .spacing(8),
                    );
                    content = content.push(connecting);
                }
                NewConnectionState::Failure(access_point) => {
                    let id = padded_control(
                        row::with_children([
                            Element::from(
                                icon::from_name("network-wireless-error-symbolic")
                                    .size(24)
                                    .symbolic(true),
                            ),
                            text::body(access_point.ssid.as_ref()).into(),
                        ])
                        .align_y(Alignment::Center)
                        .spacing(12),
                    )
                    .align_x(Alignment::Center);
                    content = content.push(id);
                    let col = padded_control(
                        column::with_children([
                            Element::from(text(fl!("unable-to-connect"))),
                            text(fl!("check-wifi-connection")).into(),
                            row::with_children([
                                Element::from(
                                    button::standard(fl!("cancel"))
                                        .on_press(Message::CancelNewConnection),
                                ),
                                button::suggested(fl!("connect"))
                                    .on_press(Message::SelectWirelessAccessPoint(
                                        access_point.clone(),
                                    ))
                                    .into(),
                            ])
                            .spacing(24)
                            .into(),
                        ])
                        .spacing(16)
                        .align_x(Alignment::Center),
                    )
                    .align_x(Alignment::Center);
                    content = content.push(col);
                }
            }
        } else {
            let list_col = self
                .nm_state
                .nm_state
                .wireless_access_points
                .iter()
                .filter(|ap| {
                    let among_active = self.nm_state.nm_state.active_conns.iter().any(|a| {
                        let hw_address = active_conn_hw_address(a);
                        ap.ssid.as_ref() == a.name() && ap.hw_address == hw_address
                    });
                    let among_known =
                        self.nm_state
                            .nm_state
                            .known_access_points
                            .iter()
                            .any(|known_ap| {
                                ap.network_type == known_ap.network_type
                                    && ap.hw_address == known_ap.hw_address
                                    && ap.ssid == known_ap.ssid
                            });
                    !among_active && !among_known
                })
                .map(|ap| {
                    let button = menu_button(
                        row::with_children([
                            Element::from(
                                icon::from_name(wifi_icon(ap.strength))
                                    .size(16)
                                    .symbolic(true),
                            ),
                            text::body(ap.ssid.as_ref())
                                .align_y(Alignment::Center)
                                .into(),
                        ])
                        .align_y(Alignment::Center)
                        .spacing(12),
                    )
                    .on_press(Message::SelectWirelessAccessPoint(ap.clone()));
                    button.into()
                });
            content = content.push(
                scrollable::<'_, Message>(column::with_children(list_col))
                    .height(Length::Fixed(300.0)),
            );
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
        activation_token_subscription(0).map(Message::Token)
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}

fn active_conn_hw_address(conn: &ActiveConnectionInfo) -> HwAddress {
    match conn {
        ActiveConnectionInfo::Wired { hw_address, .. }
        | ActiveConnectionInfo::WiFi { hw_address, .. } => {
            HwAddress::from_str(hw_address).unwrap_or_default()
        }
        ActiveConnectionInfo::Vpn { .. } => HwAddress::default(),
    }
}
