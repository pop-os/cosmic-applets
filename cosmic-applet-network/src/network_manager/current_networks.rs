// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic_dbus_networkmanager::{
    active_connection::ActiveConnection,
    device::SpecificDevice,
    interface::enums::{ApFlags, ApSecurityFlags},
};
use std::net::IpAddr;

pub async fn active_connections(
    active_connections: Vec<ActiveConnection<'_>>,
) -> zbus::Result<Vec<ActiveConnectionInfo>> {
    let mut info = Vec::<ActiveConnectionInfo>::with_capacity(active_connections.len());
    for connection in active_connections {
        if connection.vpn().await.unwrap_or_default() {
            let mut ip_addresses = Vec::new();
            for address_data in connection
                .ip4_config()
                .await?
                .address_data()
                .await
                .unwrap_or_default()
            {
                ip_addresses.push(IpAddr::V4(address_data.address));
            }
            for address_data in connection
                .ip6_config()
                .await?
                .address_data()
                .await
                .unwrap_or_default()
            {
                ip_addresses.push(IpAddr::V6(address_data.address));
            }
            info.push(ActiveConnectionInfo::Vpn {
                name: connection.id().await?,
                ip_addresses,
            });
            continue;
        }
        for device in connection.devices().await.unwrap_or_default() {
            let mut ip_addresses = Vec::new();
            for address_data in connection
                .ip4_config()
                .await?
                .address_data()
                .await
                .unwrap_or_default()
            {
                ip_addresses.push(IpAddr::V4(address_data.address));
            }
            for address_data in connection
                .ip6_config()
                .await?
                .address_data()
                .await
                .unwrap_or_default()
            {
                ip_addresses.push(IpAddr::V6(address_data.address));
            }
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
                        ip_addresses,
                    });
                }
                Some(SpecificDevice::Wireless(wireless_device)) => {
                    if let Ok(access_point) = wireless_device.active_access_point().await {
                        info.push(ActiveConnectionInfo::WiFi {
                            name: String::from_utf8_lossy(&access_point.ssid().await?).into_owned(),
                            ip_addresses,
                            hw_address: wireless_device.hw_address().await?,
                            flags: access_point.flags().await?,
                            rsn_flags: access_point.rsn_flags().await?,
                            wpa_flags: access_point.wpa_flags().await?,
                        });
                    }
                }
                Some(SpecificDevice::WireGuard(_)) => {
                    info.push(ActiveConnectionInfo::Vpn {
                        name: connection.id().await?,
                        ip_addresses,
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
        ip_addresses: Vec<IpAddr>,
    },
    WiFi {
        name: String,
        ip_addresses: Vec<IpAddr>,
        hw_address: String,
        flags: ApFlags,
        rsn_flags: ApSecurityFlags,
        wpa_flags: ApSecurityFlags,
    },
    Vpn {
        name: String,
        ip_addresses: Vec<IpAddr>,
    },
}

impl ActiveConnectionInfo {
    pub fn name(&self) -> String {
        match &self {
            ActiveConnectionInfo::Wired { name, .. } => name.clone(),
            ActiveConnectionInfo::WiFi { name, .. } => name.clone(),
            ActiveConnectionInfo::Vpn { name, .. } => name.clone(),
        }
    }
}
