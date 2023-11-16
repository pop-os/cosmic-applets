pub mod active_conns;
pub mod available_wifi;
pub mod current_networks;
pub mod devices;
pub mod wireless_enabled;

use std::{collections::HashMap, fmt::Debug, ops::Deref, time::Duration};

use cosmic::iced::{self, subscription};
use cosmic_dbus_networkmanager::{
    device::SpecificDevice,
    interface::{
        active_connection::ActiveConnectionProxy,
        enums::DeviceType,
        enums::{self, ActiveConnectionState},
    },
    nm::NetworkManager,
    settings::{connection::Settings, NetworkManagerSettings},
};
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    SinkExt, StreamExt,
};
use tokio::{process::Command, time::timeout};
use zbus::{
    zvariant::{self, ObjectPath, Value},
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
                        if c.id().await.unwrap_or_default() == ssid {
                            if network_manager.deactivate_connection(&c).await.is_ok() {
                                success = true;
                                if let Ok(ActiveConnectionState::Deactivated) = c.state().await {
                                    break;
                                } else {
                                    let mut changed = c.receive_state_changed().await;
                                    _ = tokio::time::timeout(Duration::from_secs(5), async move {
                                        loop {
                                            if let Some(next) = changed.next().await {
                                                if let Ok(ActiveConnectionState::Deactivated) = next
                                                    .get()
                                                    .await
                                                    .map(|p| ActiveConnectionState::from(p))
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
                    let s = match NetworkManagerSettings::new(&conn).await {
                        Ok(s) => s,
                        Err(_) => return State::Finished,
                    };

                    let mut status: Option<NetworkManagerEvent> = None;

                    // First try known connections
                    // TODO more convenient methods of managing settings
                    for c in s.list_connections().await.unwrap_or_default() {
                        let mut settings = match c.get_settings().await.ok() {
                            Some(s) => s,
                            None => continue,
                        };

                        let cur_ssid = settings
                            .get("802-11-wireless")
                            .and_then(|w| w.get("ssid"))
                            .cloned()
                            .and_then(|ssid| ssid.try_into().ok())
                            .and_then(|ssid| String::from_utf8(ssid).ok());
                        if cur_ssid.as_ref() != Some(&ssid) {
                            continue;
                        }

                        let mut secrets = match c.get_secrets("802-11-wireless-security").await {
                            Ok(s) => s,
                            _ => HashMap::from([(
                                "802-11-wireless-security".into(),
                                HashMap::from([
                                    (
                                        "psk".into(),
                                        Value::Str(password.as_str().into()).to_owned(),
                                    ),
                                    ("key-mgmt".into(), Value::Str("wpa-psk".into()).to_owned()),
                                ]),
                            )]),
                        };
                        if let Some(s) = secrets.get_mut("802-11-wireless-security") {
                            s.insert("psk".into(), Value::Str(password.clone().into()).to_owned());
                            settings.extend(secrets.into_iter());
                            let settings: HashMap<_, _> = settings
                                .iter()
                                .map(|(k, v)| {
                                    let map = (
                                        k.as_str(),
                                        v.iter()
                                            .map(|(k, v)| (k.as_str(), v.into()))
                                            .collect::<HashMap<_, _>>(),
                                    );
                                    map
                                })
                                .collect();
                            let updated = c.update(settings).await;
                            if updated.is_ok() {
                                let success = if let Ok(path) = network_manager
                                    .deref()
                                    .activate_connection(
                                        c.deref().path(),
                                        &ObjectPath::try_from("/").unwrap(),
                                        &ObjectPath::try_from("/").unwrap(),
                                    )
                                    .await
                                {
                                    // let active_conn = ActiveConnection::from(ActiveConnectionProxy::from(conn.1));
                                    let dummy = ActiveConnectionProxy::new(&conn).await.unwrap();
                                    let active = ActiveConnectionProxy::builder(&conn)
                                        .path(path)
                                        .unwrap()
                                        .destination(dummy.destination())
                                        .unwrap()
                                        .interface(dummy.interface())
                                        .unwrap()
                                        .build()
                                        .await
                                        .unwrap();
                                    let state = enums::ActiveConnectionState::from(
                                        active.state().await.unwrap_or_default(),
                                    );
                                    let s = if let enums::ActiveConnectionState::Activating = state
                                    {
                                        if let Ok(Some(s)) = timeout(
                                            Duration::from_secs(10),
                                            active.receive_state_changed().await.next(),
                                        )
                                        .await
                                        {
                                            s.get().await.unwrap_or_default().into()
                                        } else {
                                            state
                                        }
                                    } else {
                                        state
                                    };
                                    matches!(s, enums::ActiveConnectionState::Activated)
                                } else {
                                    false
                                };
                                status = Some(NetworkManagerEvent::RequestResponse {
                                    req: NetworkManagerRequest::Password(
                                        ssid.clone(),
                                        password.clone(),
                                    ),
                                    success,
                                    state: NetworkManagerState::new(&conn)
                                        .await
                                        .unwrap_or_default(),
                                });
                            }

                            break;
                        }
                    }

                    // create a connection
                    if status.is_none() {
                        for device in network_manager.devices().await.ok().unwrap_or_default() {
                            if matches!(
                                device.device_type().await.unwrap_or(DeviceType::Other),
                                DeviceType::Wifi
                            ) {
                                let conn_settings: HashMap<&str, HashMap<&str, zvariant::Value>> =
                                    HashMap::from([
                                        (
                                            "802-11-wireless",
                                            HashMap::from([(
                                                "ssid",
                                                Value::Array(ssid.as_bytes().into()),
                                            )]),
                                        ),
                                        (
                                            "connection",
                                            HashMap::from([
                                                ("id", Value::Str(ssid.as_str().into())),
                                                ("type", Value::Str("802-11-wireless".into())),
                                            ]),
                                        ),
                                        (
                                            "802-11-wireless-security",
                                            HashMap::from([
                                                ("psk", Value::Str(password.as_str().into())),
                                                ("key-mgmt", Value::Str("wpa-psk".into())),
                                            ]),
                                        ),
                                    ]);
                                let success = if let Ok((_, path)) = network_manager
                                    .add_and_activate_connection(
                                        conn_settings,
                                        device.path(),
                                        &ObjectPath::try_from("/").unwrap(),
                                    )
                                    .await
                                {
                                    let dummy = ActiveConnectionProxy::new(&conn).await.unwrap();
                                    let active = ActiveConnectionProxy::builder(&conn)
                                        .path(path)
                                        .unwrap()
                                        .destination(dummy.destination())
                                        .unwrap()
                                        .interface(dummy.interface())
                                        .unwrap()
                                        .build()
                                        .await
                                        .unwrap();
                                    let state = enums::ActiveConnectionState::from(
                                        active.state().await.unwrap_or_default(),
                                    );
                                    let s = if let enums::ActiveConnectionState::Activating = state
                                    {
                                        if let Ok(Some(s)) = timeout(
                                            Duration::from_secs(10),
                                            active.receive_state_changed().await.next(),
                                        )
                                        .await
                                        {
                                            s.get().await.unwrap_or_default().into()
                                        } else {
                                            state
                                        }
                                    } else {
                                        state
                                    };
                                    matches!(s, enums::ActiveConnectionState::Activated)
                                } else {
                                    false
                                };
                                _ = output
                                    .send(NetworkManagerEvent::RequestResponse {
                                        req: NetworkManagerRequest::Password(
                                            ssid.clone(),
                                            password.clone(),
                                        ),
                                        success,
                                        state: NetworkManagerState::new(&conn)
                                            .await
                                            .unwrap_or_default(),
                                    })
                                    .await;

                                break;
                            }
                        }
                    }

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
                    let s = match NetworkManagerSettings::new(&conn).await {
                        Ok(s) => s,
                        Err(_) => return State::Finished,
                    };
                    // find known connection with matching ssid and activate

                    for c in s.list_connections().await.unwrap_or_default() {
                        let settings = match c.get_settings().await.ok() {
                            Some(s) => s,
                            None => continue,
                        };

                        let cur_ssid = settings
                            .get("802-11-wireless")
                            .and_then(|w| w.get("ssid"))
                            .cloned()
                            .and_then(|ssid| ssid.try_into().ok())
                            .and_then(|ssid| String::from_utf8(ssid).ok());

                        if cur_ssid.as_ref() != Some(&ssid) {
                            continue;
                        }

                        let success = if let Ok(path) = network_manager
                            .deref()
                            .activate_connection(
                                c.deref().path(),
                                &ObjectPath::try_from("/").unwrap(),
                                &ObjectPath::try_from("/").unwrap(),
                            )
                            .await
                        {
                            let dummy = ActiveConnectionProxy::new(&conn).await.unwrap();
                            let active = ActiveConnectionProxy::builder(&conn)
                                .path(path)
                                .unwrap()
                                .destination(dummy.destination())
                                .unwrap()
                                .interface(dummy.interface())
                                .unwrap()
                                .build()
                                .await
                                .unwrap();
                            let mut state = enums::ActiveConnectionState::from(
                                active.state().await.unwrap_or_default(),
                            );
                            while let enums::ActiveConnectionState::Activating = state {
                                if let Ok(Some(s)) = timeout(
                                    Duration::from_secs(20),
                                    active.receive_state_changed().await.next(),
                                )
                                .await
                                {
                                    state = s.get().await.unwrap_or_default().into();
                                } else {
                                    break;
                                }
                            }
                            matches!(state, enums::ActiveConnectionState::Activated)
                        } else {
                            false
                        };
                        _ = output
                            .send(NetworkManagerEvent::RequestResponse {
                                req: NetworkManagerRequest::SelectAccessPoint(ssid.clone()),
                                success,
                                state: NetworkManagerState::new(&conn).await.unwrap_or_default(),
                            })
                            .await;

                        break;
                    }
                    _ = output
                        .send(NetworkManagerEvent::RequestResponse {
                            req: NetworkManagerRequest::SelectAccessPoint(ssid.clone()),
                            success: false,
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

#[derive(Debug, Clone, Default)]
pub struct NetworkManagerState {
    pub wireless_access_points: Vec<AccessPoint>,
    pub active_conns: Vec<ActiveConnectionInfo>,
    pub known_access_points: Vec<AccessPoint>,
    pub wifi_enabled: bool,
    pub airplane_mode: bool,
}

impl NetworkManagerState {
    pub async fn new(conn: &Connection) -> anyhow::Result<Self> {
        let network_manager = NetworkManager::new(conn).await?;
        let mut _self = Self::default();
        // airplane mode
        let airplaine_mode = Command::new("rfkill")
            .arg("list")
            .arg("bluetooth")
            .output()
            .await?;
        let airplane_mode = std::str::from_utf8(&airplaine_mode.stdout).unwrap_or_default();
        _self.wifi_enabled = network_manager.wireless_enabled().await.unwrap_or_default();
        _self.airplane_mode = airplane_mode.contains("Soft blocked: yes") && !_self.wifi_enabled;

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
        _self.wireless_access_points = wireless_access_points;
        _self.active_conns = active_conns;
        _self.known_access_points = known_access_points;
        Ok(_self)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.active_conns = Vec::new();
        self.known_access_points = Vec::new();
        self.wireless_access_points = Vec::new();
    }
}
