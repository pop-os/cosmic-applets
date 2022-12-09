pub mod available_wifi;
pub mod current_networks;

use std::{fmt::Debug, hash::Hash};

use cosmic::iced::{self, subscription};
use cosmic_dbus_networkmanager::{device::SpecificDevice, nm::NetworkManager};
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
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
            let (tx, rx) = unbounded();
            let active_conns = active_connections(
                network_manager
                    .active_connections()
                    .await
                    .unwrap_or_default(),
            )
            .await
            .unwrap_or_default();
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
                wireless_access_points.append(&mut f.await);
            }
            wireless_access_points.sort_by(|a, b| a.strength.cmp(&b.strength));
            drop(network_manager);
            return (
                Some((
                    id,
                    NetworkManagerEvent::Init {
                        sender: tx,
                        wireless_access_points,
                        wifi_enabled,
                        airplane_mode: false,
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
            let mut active_conns_changed =
                network_manager.receive_active_connections_changed().await;
            let mut devices_changed = network_manager.receive_devices_changed().await;
            let mut networking_enabled_changed =
                network_manager.receive_networking_enabled_changed().await;
            let mut req = rx.next().boxed().fuse();

            let (update, should_exit) = futures::select! {
                req = req => {match req {
                    Some(NetworkManagerRequest::SetAirplaneMode(_)) => {
                        // TODO set airplane mode
                        // let _ = network_manager.set_wireless_enabled(state).await;
                        (None, false)
                    }
                    Some(NetworkManagerRequest::SetWiFi(enabled)) => {
                        let _ = network_manager.set_wireless_enabled(enabled).await;
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
                    wireless_access_points.sort_by(|a, b| a.strength.cmp(&b.strength));
                    (Some((id, NetworkManagerEvent::WirelessAccessPoints(wireless_access_points))), false)
                }
                enabled = networking_enabled_changed.next().boxed().fuse() => {
                    let update = if let Some(update) = enabled {
                        update.get().await.ok().map(|update| (id, NetworkManagerEvent::WiFiEnabled(update)))
                    } else {
                        None
                    };
                    (update, false)
                }
            };
            drop(active_conns_changed);
            drop(networking_enabled_changed);
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

#[derive(Debug)]
pub enum NetworkManagerRequest {
    SetAirplaneMode(bool),
    SetWiFi(bool),
    // TODO select wifi
}

#[derive(Debug, Clone)]
pub enum NetworkManagerEvent {
    Init {
        sender: UnboundedSender<NetworkManagerRequest>,
        wireless_access_points: Vec<AccessPoint>,
        active_conns: Vec<ActiveConnectionInfo>,
        wifi_enabled: bool,
        airplane_mode: bool,
    },
    WiFiEnabled(bool),
    WirelessAccessPoints(Vec<AccessPoint>),
    ActiveConns(Vec<ActiveConnectionInfo>),
}
