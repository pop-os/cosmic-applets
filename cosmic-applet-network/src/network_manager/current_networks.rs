// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic_dbus_networkmanager::{
    active_connection::ActiveConnection, device::SpecificDevice,
    interface::enums::ActiveConnectionState,
};
use std::net::Ipv4Addr;

use super::hw_address::HwAddress;

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
                        hw_address: HwAddress::from_str(&wired_device.hw_address().await?)
                            .unwrap_or_default(),
                        speed: wired_device.speed().await?,
                        ip_addresses: addresses.clone(),
                    });
                }
                Some(SpecificDevice::Wireless(wireless_device)) => {
                    if let Ok(access_point) = wireless_device.active_access_point().await {
                        info.push(ActiveConnectionInfo::WiFi {
                            name: String::from_utf8_lossy(&access_point.ssid().await?).into_owned(),
                            ip_addresses: addresses.clone(),
                            hw_address: HwAddress::from_str(&wireless_device.hw_address().await?)
                                .unwrap_or_default(),
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

    info.sort_unstable();
    Ok(info)
}

#[derive(Debug, Clone)]
pub enum ActiveConnectionInfo {
    Wired {
        name: String,
        hw_address: HwAddress,
        speed: u32,
        ip_addresses: Vec<Ipv4Addr>,
    },
    WiFi {
        name: String,
        ip_addresses: Vec<Ipv4Addr>,
        hw_address: HwAddress,
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
            Self::Wired { name, .. } | Self::WiFi { name, .. } | Self::Vpn { name, .. } => {
                name.clone()
            }
        }
    }
    pub fn hw_address(&self) -> HwAddress {
        match &self {
            Self::Wired { hw_address, .. } | Self::WiFi { hw_address, .. } => *hw_address,
            Self::Vpn { .. } => HwAddress::default(),
        }
    }
}

impl std::cmp::Ord for ActiveConnectionInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Vpn { .. }, Self::Wired { .. } | Self::WiFi { .. })
            | (Self::Wired { .. }, Self::WiFi { .. }) => std::cmp::Ordering::Less,

            (Self::WiFi { .. }, Self::Wired { .. } | Self::Vpn { .. })
            | (Self::Wired { .. }, Self::Vpn { .. }) => std::cmp::Ordering::Greater,

            (Self::Vpn { name: n1, .. }, Self::Vpn { name: n2, .. })
            | (Self::Wired { name: n1, .. }, Self::Wired { name: n2, .. })
            | (Self::WiFi { name: n1, .. }, Self::WiFi { name: n2, .. }) => n1.cmp(n2),
        }
    }
}

impl std::cmp::Eq for ActiveConnectionInfo {}

impl std::cmp::PartialOrd for ActiveConnectionInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::cmp::PartialEq for ActiveConnectionInfo {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Wired {
                    name: n1,
                    hw_address: a1,
                    ..
                },
                Self::Wired {
                    name: n2,
                    hw_address: a2,
                    ..
                },
            )
            | (
                Self::WiFi {
                    name: n1,
                    hw_address: a1,
                    ..
                },
                Self::WiFi {
                    name: n2,
                    hw_address: a2,
                    ..
                },
            ) => n1 == n2 && a1 == a2,

            (Self::Vpn { name: n1, .. }, Self::Vpn { name: n2, .. }) => n1 == n2,

            _ => false,
        }
    }
}
