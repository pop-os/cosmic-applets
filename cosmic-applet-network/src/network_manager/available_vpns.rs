// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic_dbus_networkmanager::settings::{NetworkManagerSettings, connection::Settings};
use zbus::Connection;

#[derive(Debug, Clone)]
pub struct VpnConnection {
    pub name: String,
    pub uuid: String,
}

/// Load all available VPN connections from NetworkManager settings
pub async fn load_vpn_connections(conn: &Connection) -> anyhow::Result<Vec<VpnConnection>> {
    let nm_settings = NetworkManagerSettings::new(conn).await?;
    let connections = nm_settings.list_connections().await?;

    let mut vpn_connections = Vec::new();

    for connection in connections {
        let settings_map = match connection.get_settings().await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let settings = Settings::new(settings_map);

        // Check if this is a VPN connection
        if let Some(connection_settings) = &settings.connection {
            if let Some(conn_type) = &connection_settings.type_ {
                // VPN connections have type "vpn" or "wireguard"
                if conn_type == "vpn" || conn_type == "wireguard" {
                    let name = connection_settings.id.clone().unwrap_or_else(|| "Unknown VPN".to_string());
                    let uuid = connection_settings.uuid.clone().unwrap_or_default();

                    vpn_connections.push(VpnConnection { name, uuid });
                }
            }
        }
    }

    // Sort by name for consistent UI
    vpn_connections.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(vpn_connections)
}
