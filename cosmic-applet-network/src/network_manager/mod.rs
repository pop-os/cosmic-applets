pub mod active_conns;
pub mod available_wifi;
pub mod current_networks;
pub mod devices;
pub mod wireless_enabled;

use std::{collections::HashMap, fmt::Debug, time::Duration};

use cosmic::iced::{self, subscription};
use cosmic_dbus_networkmanager::{
    active_connection::ActiveConnection,
    device::SpecificDevice,
    interface::{
        active_connection::ActiveConnectionProxy,
        enums::{self, ActiveConnectionState, DeviceType, NmConnectivityState},
    },
    nm::NetworkManager,
    settings::{connection::Settings, NetworkManagerSettings},
};
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    SinkExt, StreamExt,
};
use tokio::process::Command;
use zbus::{
    zvariant::{self, OwnedValue, Value},
    Connection,
};

use self::{
    available_wifi::{handle_wireless_device, AccessPoint},
    current_networks::{active_connections, ActiveConnectionInfo},
};

#[derive(Debug)]
pub enum State {
    Ready,
    Waiting(Connection, UnboundedReceiver<NetworkManagerRequest>),
    Finished,
}

pub fn network_manager_subscription<I: Copy + Debug + std::hash::Hash + 'static>(
    id: I,
) -> iced::Subscription<NetworkManagerEvent> {
    subscription::channel(id, 50, |mut output| async move {
        let mut state = State::Ready;

        loop {
            state = start_listening(state, &mut output).await;
        }
    })
}

async fn start_listening(
    state: State,
    output: &mut futures::channel::mpsc::Sender<NetworkManagerEvent>,
) -> State {
    match state {
        State::Ready => {
            let conn = match Connection::system().await {
                Ok(c) => c,
                Err(_) => return State::Finished,
            };

            let (tx, rx) = unbounded();
            let nm_state = NetworkManagerState::new(&conn).await.unwrap_or_default();
            if output
                .send(NetworkManagerEvent::Init {
                    conn: conn.clone(),
                    sender: tx,
                    state: nm_state,
                })
                .await
                .is_ok()
            {
                State::Waiting(conn, rx)
            } else {
                State::Finished
            }
        }
        State::Waiting(conn, mut rx) => {
            let network_manager = match NetworkManager::new(&conn).await {
                Ok(n) => n,
                Err(_) => return State::Finished,
            };

            match rx.next().await {
                Some(NetworkManagerRequest::Disconnect(ssid)) => {
                    let mut success = false;
                    for c in network_manager
                        .active_connections()
                        .await
                        .unwrap_or_default()
                    {
                        if c.id().await.unwrap_or_default() == ssid
                            && network_manager.deactivate_connection(&c).await.is_ok()
                        {
                            success = true;
                            if let Ok(ActiveConnectionState::Deactivated) = c.state().await {
                                break;
                            } else {
                                let mut changed = c.receive_state_changed().await;
                                _ = tokio::time::timeout(Duration::from_secs(5), async move {
                                    loop {
                                        if let Some(next) = changed.next().await {
                                            if let Ok(ActiveConnectionState::Deactivated) =
                                                next.get().await.map(ActiveConnectionState::from)
                                            {
                                                break;
                                            }
                                        }
                                    }
                                })
                                .await;
                            }
                            break;
                        }
                    }
                    _ = output
                        .send(NetworkManagerEvent::RequestResponse {
                            req: NetworkManagerRequest::Disconnect(ssid.clone()),
                            success,
                            state: NetworkManagerState::new(&conn).await.unwrap_or_default(),
                        })
                        .await;
                }
                Some(NetworkManagerRequest::SetAirplaneMode(airplane_mode)) => {
                    // wifi
                    let mut success = network_manager
                        .set_wireless_enabled(!airplane_mode)
                        .await
                        .is_ok();
                    // bluetooth
                    success = success
                        && Command::new("rfkill")
                            .arg(if airplane_mode { "block" } else { "unblock" })
                            .arg("bluetooth")
                            .output()
                            .await
                            .is_ok();
                    let mut state = NetworkManagerState::new(&conn).await.unwrap_or_default();
                    state.airplane_mode = if success {
                        airplane_mode
                    } else {
                        !airplane_mode
                    };
                    if state.airplane_mode {
                        state.wifi_enabled = false;
                    }
                    _ = output
                        .send(NetworkManagerEvent::RequestResponse {
                            req: NetworkManagerRequest::SetAirplaneMode(airplane_mode),
                            success,
                            state,
                        })
                        .await;
                }
                Some(NetworkManagerRequest::SetWiFi(enabled)) => {
                    let success = network_manager.set_wireless_enabled(enabled).await.is_ok();
                    let mut state = NetworkManagerState::new(&conn).await.unwrap_or_default();
                    state.wifi_enabled = if success { enabled } else { !enabled };
                    let response = NetworkManagerEvent::RequestResponse {
                        req: NetworkManagerRequest::SetWiFi(enabled),
                        success,
                        state,
                    };
                    _ = output.send(response).await;
                }
                Some(NetworkManagerRequest::Password(ssid, password)) => {
                    let nm_state = NetworkManagerState::new(&conn).await.unwrap_or_default();
                    let success = nm_state
                        .connect_wifi(&conn, &ssid, Some(&password))
                        .await
                        .is_ok();

                    let status = Some(NetworkManagerEvent::RequestResponse {
                        req: NetworkManagerRequest::Password(ssid.clone(), password.clone()),
                        success,
                        state: NetworkManagerState::new(&conn).await.unwrap_or_default(),
                    });

                    if let Some(e) = status {
                        _ = output.send(e).await;
                    } else {
                        _ = output
                            .send(NetworkManagerEvent::RequestResponse {
                                req: NetworkManagerRequest::Password(ssid, password),
                                success: false,
                                state: NetworkManagerState::new(&conn).await.unwrap_or_default(),
                            })
                            .await;
                    }
                }
                Some(NetworkManagerRequest::SelectAccessPoint(ssid)) => {
                    let state = NetworkManagerState::new(&conn).await.unwrap_or_default();
                    let success = if let Err(err) = state.connect_wifi(&conn, &ssid, None).await {
                        tracing::error!("Failed to connect to access point: {:?}", err);
                        false
                    } else {
                        true
                    };

                    _ = output
                        .send(NetworkManagerEvent::RequestResponse {
                            req: NetworkManagerRequest::SelectAccessPoint(ssid.clone()),
                            success,
                            state: NetworkManagerState::new(&conn).await.unwrap_or_default(),
                        })
                        .await;
                }
                Some(NetworkManagerRequest::Reload) => {
                    let state = NetworkManagerState::new(&conn).await.unwrap_or_default();
                    _ = output
                        .send(NetworkManagerEvent::RequestResponse {
                            req: NetworkManagerRequest::Reload,
                            success: true,
                            state,
                        })
                        .await;
                }
                _ => {
                    return State::Finished;
                }
            };

            State::Waiting(conn, rx)
        }
        State::Finished => iced::futures::future::pending().await,
    }
}

#[derive(Debug, Clone)]
pub enum NetworkManagerRequest {
    SetAirplaneMode(bool),
    SetWiFi(bool),
    SelectAccessPoint(String),
    Disconnect(String),
    Password(String, String),
    Reload,
}

#[derive(Debug, Clone)]
pub enum NetworkManagerEvent {
    RequestResponse {
        req: NetworkManagerRequest,
        state: NetworkManagerState,
        success: bool,
    },
    Init {
        conn: Connection,
        sender: UnboundedSender<NetworkManagerRequest>,
        state: NetworkManagerState,
    },
    WiFiEnabled(NetworkManagerState),
    WirelessAccessPoints(NetworkManagerState),
    ActiveConns(NetworkManagerState),
}

#[derive(Debug, Clone)]
pub struct NetworkManagerState {
    pub wireless_access_points: Vec<AccessPoint>,
    pub active_conns: Vec<ActiveConnectionInfo>,
    pub known_access_points: Vec<AccessPoint>,
    pub wifi_enabled: bool,
    pub airplane_mode: bool,
    pub connectivity: NmConnectivityState,
}

impl Default for NetworkManagerState {
    fn default() -> Self {
        Self {
            wireless_access_points: Vec::new(),
            active_conns: Vec::new(),
            known_access_points: Vec::new(),
            wifi_enabled: false,
            airplane_mode: false,
            connectivity: NmConnectivityState::Unknown,
        }
    }
}

impl NetworkManagerState {
    pub async fn new(conn: &Connection) -> anyhow::Result<Self> {
        let network_manager = NetworkManager::new(conn).await?;
        let mut self_ = Self::default();
        // airplane mode
        let airplaine_mode = Command::new("rfkill")
            .arg("list")
            .arg("bluetooth")
            .output()
            .await?;
        let airplane_mode = std::str::from_utf8(&airplaine_mode.stdout).unwrap_or_default();
        self_.wifi_enabled = network_manager.wireless_enabled().await.unwrap_or_default();
        self_.airplane_mode = airplane_mode.contains("Soft blocked: yes") && !self_.wifi_enabled;

        let s = NetworkManagerSettings::new(conn).await?;
        _ = s.load_connections(&[]).await;
        let known_conns = s.list_connections().await.unwrap_or_default();
        let mut active_conns = active_connections(
            network_manager
                .active_connections()
                .await
                .unwrap_or_default(),
        )
        .await
        .unwrap_or_default();
        active_conns.sort_by(|a, b| {
            let helper = |conn: &ActiveConnectionInfo| match conn {
                ActiveConnectionInfo::Vpn { name, .. } => format!("0{name}"),
                ActiveConnectionInfo::Wired { name, .. } => format!("1{name}"),
                ActiveConnectionInfo::WiFi { name, .. } => format!("2{name}"),
            };
            helper(a).cmp(&helper(b))
        });
        let devices = network_manager.devices().await.ok().unwrap_or_default();
        let wireless_access_point_futures: Vec<_> = devices
            .into_iter()
            .map(|device| async move {
                if let Ok(Some(SpecificDevice::Wireless(wireless_device))) =
                    device.downcast_to_device().await
                {
                    handle_wireless_device(wireless_device)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            })
            .collect();
        let mut wireless_access_points = Vec::with_capacity(wireless_access_point_futures.len());
        for f in wireless_access_point_futures {
            let mut access_points = f.await;
            wireless_access_points.append(&mut access_points);
        }
        let mut known_ssid = Vec::with_capacity(known_conns.len());
        for c in known_conns {
            let s = c.get_settings().await.unwrap();
            let s = Settings::new(s);
            if let Some(cur_ssid) = s
                .wifi
                .clone()
                .and_then(|w| w.ssid)
                .and_then(|ssid| String::from_utf8(ssid).ok())
            {
                known_ssid.push(cur_ssid);
            }
        }
        let known_access_points: Vec<_> = wireless_access_points
            .iter()
            .filter(|a| {
                known_ssid.contains(&a.ssid) && !active_conns.iter().any(|ac| ac.name() == a.ssid)
            })
            .cloned()
            .collect();
        wireless_access_points.sort_by(|a, b| b.strength.cmp(&a.strength));
        self_.wireless_access_points = wireless_access_points;
        self_.active_conns = active_conns;
        self_.known_access_points = known_access_points;
        self_.connectivity = network_manager.connectivity().await?;

        Ok(self_)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.active_conns = Vec::new();
        self.known_access_points = Vec::new();
        self.wireless_access_points = Vec::new();
    }

    async fn connect_wifi<'a>(
        &self,
        conn: &Connection,
        ssid: &str,
        password: Option<&str>,
    ) -> anyhow::Result<()> {
        let nm = NetworkManager::new(conn).await?;

        for c in nm.active_connections().await.unwrap_or_default() {
            if self
                .wireless_access_points
                .iter()
                .any(|w| Ok(Some(w.ssid.clone())) == c.cached_id())
            {
                _ = nm.deactivate_connection(&c).await;
            }
        }

        let Some(ap) = self
            .wireless_access_points
            .iter()
            .find(|ap| ap.ssid == ssid)
        else {
            return Err(anyhow::anyhow!("Access point not found"));
        };

        let mut conn_settings: HashMap<&str, HashMap<&str, zvariant::Value>> = HashMap::from([
            (
                "802-11-wireless",
                HashMap::from([("ssid", Value::Array(ssid.as_bytes().into()))]),
            ),
            (
                "connection",
                HashMap::from([
                    ("id", Value::Str(ssid.into())),
                    ("type", Value::Str("802-11-wireless".into())),
                ]),
            ),
        ]);

        if let Some(pass) = password {
            conn_settings.insert(
                "802-11-wireless-security",
                HashMap::from([
                    ("psk", Value::Str(pass.into())),
                    ("key-mgmt", Value::Str("wpa-psk".into())),
                ]),
            );
        }

        let devices = nm.devices().await?;
        for device in devices {
            if !matches!(
                device.device_type().await.unwrap_or(DeviceType::Other),
                DeviceType::Wifi
            ) {
                continue;
            }

            let s = NetworkManagerSettings::new(conn).await?;
            let known_conns = s.list_connections().await.unwrap_or_default();
            let mut known_conn = None;
            for c in known_conns {
                let settings = c.get_settings().await.ok().unwrap_or_default();
                let s = Settings::new(settings);
                if let Some(cur_ssid) = s
                    .wifi
                    .clone()
                    .and_then(|w| w.ssid)
                    .and_then(|ssid| String::from_utf8(ssid).ok())
                {
                    if cur_ssid == ssid {
                        known_conn = Some(c);
                        break;
                    }
                }
            }
            let active_conn = if let Some(known_conn) = known_conn.as_ref() {
                // update settings if needed
                let mut settings = known_conn.get_settings().await.ok().unwrap_or_default();
                settings.extend(conn_settings.iter().map(|s| {
                    let map = (
                        s.0.to_string(),
                        s.1.iter()
                            .filter_map(|(k, v)| {
                                OwnedValue::try_from(v).map(|v| (k.to_string(), v)).ok()
                            })
                            .collect::<HashMap<_, _>>(),
                    );
                    map
                }));

                known_conn.update(conn_settings).await?;

                nm.activate_connection(known_conn, &device).await?
            } else {
                let (_, active_conn) = nm
                    .add_and_activate_connection(conn_settings, device.inner().path(), &ap.path)
                    .await?;
                let dummy = ActiveConnectionProxy::new(&conn, active_conn).await?;
                let active = ActiveConnectionProxy::builder(&conn)
                    .destination(dummy.inner().destination().to_owned())
                    .unwrap()
                    .interface(dummy.inner().interface().to_owned())
                    .unwrap()
                    .build()
                    .await
                    .unwrap();
                ActiveConnection::from(active)
            };
            let mut state =
                enums::ActiveConnectionState::from(active_conn.state().await.unwrap_or_default());
            return match state {
                ActiveConnectionState::Activating => {
                    if let Ok(Some(s)) = tokio::time::timeout(
                        Duration::from_secs(20),
                        active_conn.receive_state_changed().await.next(),
                    )
                    .await
                    {
                        state = s.get().await.unwrap_or_default().into();
                        if matches!(state, enums::ActiveConnectionState::Activated) {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("Failed to activate connection"))
                        }
                    } else {
                        Err(anyhow::anyhow!("Failed to activate connection"))
                    }
                }
                ActiveConnectionState::Activated => Ok(()),
                _ => Err(anyhow::anyhow!("Failed to activate connection")),
            };
        }

        Err(anyhow::anyhow!("No wifi device found"))
    }
}
