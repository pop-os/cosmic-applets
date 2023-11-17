// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic_dbus_networkmanager::{
    active_connection::ActiveConnection, device::SpecificDevice,
    interface::enums::ActiveConnectionState,
};
use std::net::Ipv4Addr;

pub async fn active_connections(
    active_connections: Vec<ActiveConnection<'_>>,
) -> zbus::Result<Vec<ActiveConnectionInfo>> {
    let mut info = Vec::<ActiveConnectionInfo>::with_capacity(active_connections.len());
    for connection in active_connections {
        let ipv4 = connection
            .ip4_config()
            .await?
            .address_data()
            .await
            .unwrap_or_default();
        let addresses: Vec<_> = ipv4.iter().map(|d| d.address).collect();
        let state = connection
            .state()
            .await
            .unwrap_or(ActiveConnectionState::Unknown);

        if connection.vpn().await.unwrap_or_default() {
            info.push(ActiveConnectionInfo::Vpn {
                name: connection.id().await?,
                ip_addresses: addresses.clone(),
            });
            continue;
        }
        for device in connection.devices().await.unwrap_or_default() {
            match device
                .downcast_to_device()
                .await
                .ok()
                .and_then(|inner| inner)
            {
                Some(SpecificDevice::Wired(wired_device)) => {
                    info.push(ActiveConnectionInfo::Wired {
                        name: connection.id().await?,
                        hw_address: wired_device.hw_address().await?,
                        speed: wired_device.speed().await?,
                        ip_addresses: addresses.clone(),
                    });
                }
                Some(SpecificDevice::Wireless(wireless_device)) => {
                    if let Ok(access_point) = wireless_device.active_access_point().await {
                        info.push(ActiveConnectionInfo::WiFi {
                            name: String::from_utf8_lossy(&access_point.ssid().await?).into_owned(),
                            ip_addresses: addresses.clone(),
                            hw_address: wireless_device.hw_address().await?,
                            state,
                            strength: access_point.strength().await.unwrap_or_default(),
                        });
                    }
                }
                Some(SpecificDevice::WireGuard(_)) => {
                    info.push(ActiveConnectionInfo::Vpn {
                        name: connection.id().await?,
                        ip_addresses: addresses.clone(),
                    });
                }
                _ => {}
            }
        }
    }

    info.sort_by(|a, b| {
        let helper = |conn: &ActiveConnectionInfo| match conn {
            ActiveConnectionInfo::Vpn { name, .. } => format!("0{name}"),
            ActiveConnectionInfo::Wired { name, .. } => format!("1{name}"),
            ActiveConnectionInfo::WiFi { name, .. } => format!("2{name}"),
        };
        helper(a).cmp(&helper(b))
    });

    Ok(info)
}

#[derive(Debug, Clone)]
pub enum ActiveConnectionInfo {
    Wired {
        name: String,
        hw_address: String,
        speed: u32,
        ip_addresses: Vec<Ipv4Addr>,
    },
    WiFi {
        name: String,
        ip_addresses: Vec<Ipv4Addr>,
        hw_address: String,
        state: ActiveConnectionState,
        strength: u8,
    },
    Vpn {
        name: String,
        ip_addresses: Vec<Ipv4Addr>,
    },
}

impl ActiveConnectionInfo {
    pub fn name(&self) -> String {
        match &self {
            Self::Wired { name, .. } => name.clone(),
            Self::WiFi { name, .. } => name.clone(),
            Self::Vpn { name, .. } => name.clone(),
        }
    }
}
