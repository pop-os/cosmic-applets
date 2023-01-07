pub mod available_wifi;
pub mod current_networks;

use std::{fmt::Debug, hash::Hash, time::Duration};

use cosmic::iced::{self, subscription};
use cosmic_dbus_networkmanager::{
    device::SpecificDevice,
    interface::{enums::DeviceType, settings::connection::ConnectionSettingsProxy},
    nm::NetworkManager,
    settings::{
        connection::{ConnectionSettings, Secrets, Settings},
        NetworkManagerSettings,
    },
};
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    future::ok,
    FutureExt, StreamExt,
};
use zbus::Connection;

use self::{
    available_wifi::{handle_wireless_device, AccessPoint},
    current_networks::{active_connections, ActiveConnectionInfo},
};

// TODO subscription for wifi list & selection of wifi
// TODO subscription & channel for enabling / disabling wifi
// TODO subscription for displaying active connections & devices

pub fn network_manager_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced::Subscription<(I, NetworkManagerEvent)> {
    subscription::unfold(id, State::Ready, move |state| start_listening(id, state))
}

#[derive(Debug)]
pub enum State {
    Ready,
    Waiting(Connection, UnboundedReceiver<NetworkManagerRequest>),
    Finished,
}

async fn start_listening<I: Copy>(
    id: I,
    state: State,
) -> (Option<(I, NetworkManagerEvent)>, State) {
    match state {
        State::Ready => {
            let conn = match Connection::system().await {
                Ok(c) => c,
                Err(_) => return (None, State::Finished),
            };
            let network_manager = match NetworkManager::new(&conn).await {
                Ok(n) => n,
                Err(_) => return (None, State::Finished),
            };
            let s = match NetworkManagerSettings::new(&conn).await {
                Ok(s) => s,
                Err(_) => return (None, State::Finished),
            };
            let known_conns = s.list_connections().await.unwrap_or_default();

            let (tx, rx) = unbounded();
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
            let wifi_enabled = network_manager.wireless_enabled().await.unwrap_or_default();
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
            let mut wireless_access_points =
                Vec::with_capacity(wireless_access_point_futures.len());
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
                    known_ssid.contains(&a.ssid)
                        && !active_conns.iter().any(|ac| ac.name() == a.ssid)
                })
                .cloned()
                .collect();
            wireless_access_points.sort_by(|a, b| b.strength.cmp(&a.strength));
            drop(network_manager);
            return (
                Some((
                    id,
                    NetworkManagerEvent::Init {
                        sender: tx,
                        wireless_access_points,
                        wifi_enabled,
                        airplane_mode: false,
                        known_access_points,
                        active_conns,
                    },
                )),
                State::Waiting(conn, rx),
            );
        }
        State::Waiting(conn, mut rx) => {
            let network_manager = match NetworkManager::new(&conn).await {
                Ok(n) => n,
                Err(_) => return (None, State::Finished),
            };

            let mut active_conns_changed = tokio::time::sleep(Duration::from_secs(5))
                .then(|_| async { network_manager.receive_active_connections_changed().await })
                .await;
            let mut devices_changed = network_manager.receive_devices_changed().await;
            let mut wireless_enabled_changed =
                network_manager.receive_wireless_enabled_changed().await;
            let mut req = rx.next().boxed().fuse();

            let (update, should_exit) = futures::select! {
                req = req => {
                    match req {
                        Some(NetworkManagerRequest::SetAirplaneMode(state)) => {
                            // TODO set airplane mode
                            let _ = network_manager.set_wireless_enabled(state).await;
                            (None, false)
                        }
                        Some(NetworkManagerRequest::SetWiFi(enabled)) => {
                            let success = network_manager.set_wireless_enabled(enabled).await.is_ok();
                            let active_conns =  active_connections(network_manager.active_connections().await.unwrap_or_default()).await.unwrap_or_default();
                            let devices = network_manager.devices().await.ok().unwrap_or_default();
                            let wireless_access_point_futures: Vec<_> = devices.into_iter().map(|device| async move {
                                if let Ok(Some(SpecificDevice::Wireless(wireless_device))) =
                                device.downcast_to_device().await
                                {
                                    handle_wireless_device(wireless_device).await.unwrap_or_default()
                                } else {
                                    Vec::new()
                                }
                            }).collect();
                            let mut wireless_access_points = Vec::with_capacity(wireless_access_point_futures.len());
                            for f in wireless_access_point_futures {
                                wireless_access_points.append(&mut f.await);
                            }
                            (Some((id, NetworkManagerEvent::RequestResponse {
                                req: NetworkManagerRequest::SetWiFi(enabled),
                                success,
                                active_conns,
                                wireless_access_points,
                                wifi_enabled: enabled,
                                airplane_mode: false,
                            })), false)
                        }
                        Some(NetworkManagerRequest::SelectAccessPoint(ssid)) => {
                            'device_loop: for device in network_manager.devices().await.ok().unwrap_or_default() {
                                if matches!(device.device_type().await.unwrap_or(DeviceType::Other), DeviceType::Wifi) {
                                    let connection_settings = NetworkManagerSettings::new(&conn).await.unwrap();
                                    for conn in connection_settings.list_connections().await.unwrap() {
                                        let s = conn.get_settings().await.unwrap();
                                        let s = Settings::new(s);

                                        let cur_ssid = s
                                            .wifi
                                            .clone()
                                            .and_then(|w| w.ssid)
                                            .and_then(|ssid| String::from_utf8(ssid).ok());
                                        if cur_ssid.as_ref() == Some(&ssid) {
                                            // dbg!(s);
                                            // dbg!(conn.get_secrets("connection").await);
                                            // dbg!(Secrets::new(&conn).await);
                                            // dbg!(psk);
                                            // connection update can be used to set password
                                        }
                                    }
                                    for conn in device.available_connections().await.unwrap_or_default() {
                                        // network_manager.activate_connection(conn, device.clone());
                                        // dbg!(&conn.path());
                                        // TODO activate connection
                                    }
                                }
                            }
                            (None, false)
                        }
                        None => {
                            (None, true)
                        }
                    }}
                _ = active_conns_changed.next().boxed().fuse() => {
                    let active_conns =  active_connections(network_manager.active_connections().await.unwrap_or_default()).await.unwrap_or_default();

                    (Some((id, NetworkManagerEvent::ActiveConns(active_conns))), false)
                }
                _ = devices_changed.next().boxed().fuse() => {
                    let devices = network_manager.devices().await.ok().unwrap_or_default();
                    let wireless_access_point_futures: Vec<_> = devices.into_iter().map(|device| async move {
                        if let Ok(Some(SpecificDevice::Wireless(wireless_device))) =
                        device.downcast_to_device().await
                        {
                            handle_wireless_device(wireless_device).await.unwrap_or_default()
                        } else {
                            Vec::new()
                        }
                    }).collect();
                    let mut wireless_access_points = Vec::with_capacity(wireless_access_point_futures.len());
                    for f in wireless_access_point_futures {
                        wireless_access_points.append(&mut f.await);
                    }
                    (Some((id, NetworkManagerEvent::WirelessAccessPoints(wireless_access_points))), false)
                }
                enabled = wireless_enabled_changed.next().boxed().fuse() => {
                    let update = if let Some(update) = enabled {
                        update.get().await.ok().map(|update| (id, NetworkManagerEvent::WiFiEnabled(update)))
                    } else {
                        None
                    };
                    (update, false)
                }
            };
            drop(active_conns_changed);
            drop(wireless_enabled_changed);
            drop(req);
            (
                update,
                if should_exit {
                    State::Finished
                } else {
                    State::Waiting(conn, rx)
                },
            )
        }
        State::Finished => iced::futures::future::pending().await,
    }
}

#[derive(Debug, Clone)]
pub enum NetworkManagerRequest {
    SetAirplaneMode(bool),
    SetWiFi(bool),
    SelectAccessPoint(String),
}

#[derive(Debug, Clone)]
pub enum NetworkManagerEvent {
    Init {
        sender: UnboundedSender<NetworkManagerRequest>,
        wireless_access_points: Vec<AccessPoint>,
        active_conns: Vec<ActiveConnectionInfo>,
        known_access_points: Vec<AccessPoint>,
        wifi_enabled: bool,
        airplane_mode: bool,
    },
    RequestResponse {
        req: NetworkManagerRequest,
        wireless_access_points: Vec<AccessPoint>,
        active_conns: Vec<ActiveConnectionInfo>,
        wifi_enabled: bool,
        airplane_mode: bool,
        success: bool,
    },
    WiFiEnabled(bool),
    WirelessAccessPoints(Vec<AccessPoint>),
    ActiveConns(Vec<ActiveConnectionInfo>),
}
