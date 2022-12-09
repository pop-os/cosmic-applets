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
        if connection.vpn().await? {
            let mut ip_addresses = Vec::new();
            for address_data in connection.ip4_config().await?.address_data().await? {
                ip_addresses.push(IpAddr::V4(address_data.address));
            }
            for address_data in connection.ip6_config().await?.address_data().await? {
                ip_addresses.push(IpAddr::V6(address_data.address));
            }
            info.push(ActiveConnectionInfo::Vpn {
                name: connection.id().await?,
                ip_addresses,
            });
            continue;
        }
        for device in connection.devices().await? {
            match device.downcast_to_device().await? {
                Some(SpecificDevice::Wired(wired_device)) => {
                    let mut ip_addresses = Vec::new();
                    for address_data in device.ip4_config().await?.address_data().await? {
                        ip_addresses.push(IpAddr::V4(address_data.address));
                    }
                    for address_data in device.ip6_config().await?.address_data().await? {
                        ip_addresses.push(IpAddr::V6(address_data.address));
                    }
                    info.push(ActiveConnectionInfo::Wired {
                        name: connection.id().await?,
                        hw_address: wired_device.hw_address().await?,
                        speed: wired_device.speed().await?,
                        ip_addresses,
                    });
                }
                Some(SpecificDevice::Wireless(wireless_device)) => {
                    let access_point = wireless_device.active_access_point().await?;
                    info.push(ActiveConnectionInfo::WiFi {
                        name: String::from_utf8_lossy(&access_point.ssid().await?).into_owned(),
                        hw_address: wireless_device.hw_address().await?,
                        flags: access_point.flags().await?,
                        rsn_flags: access_point.rsn_flags().await?,
                        wpa_flags: access_point.wpa_flags().await?,
                    });
                }
                Some(SpecificDevice::WireGuard(_)) => {
                    let mut ip_addresses = Vec::new();
                    for address_data in connection.ip4_config().await?.address_data().await? {
                        ip_addresses.push(IpAddr::V4(address_data.address));
                    }
                    for address_data in connection.ip6_config().await?.address_data().await? {
                        ip_addresses.push(IpAddr::V6(address_data.address));
                    }
                    info.push(ActiveConnectionInfo::Vpn {
                        name: connection.id().await?,
                        ip_addresses,
                    });
                }
                _ => {}
            }
        }
    }
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
