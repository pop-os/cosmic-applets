// SPDX-License-Identifier: LGPL-3.0-or-later

use cosmic_dbus_networkmanager::{
    access_point::AccessPoint,
    device::{wired::WiredDevice, SpecificDevice},
    interface::enums::{ApFlags, ApSecurityFlags},
    nm::NetworkManager,
};
use gtk4::{
    glib::{self, clone, source::PRIORITY_DEFAULT, MainContext, Sender},
    prelude::*,
};
use zbus::Connection;

pub fn add_current_networks(target: &gtk4::Box) {
    let our_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let (tx, rx) = MainContext::channel::<Vec<ActiveConnectionInfo>>(PRIORITY_DEFAULT);
    crate::task::spawn(handle_devices(tx));
    target.append(&our_box);
}

async fn handle_devices(tx: Sender<Vec<ActiveConnectionInfo>>) -> zbus::Result<()> {
    let conn = Connection::system().await?;
    let network_manager = NetworkManager::new(&conn).await?;
    loop {
        let active_connections = network_manager.active_connections().await?;
        let mut info = Vec::<ActiveConnectionInfo>::with_capacity(active_connections.len());
        for connection in active_connections {
            for device in connection.devices().await? {
                match device.downcast_to_device().await? {
                    Some(SpecificDevice::Wired(wired_device)) => {
                        info.push(ActiveConnectionInfo::Wired {
                            name: connection.id().await?,
                            hw_address: wired_device.hw_address().await?,
                            speed: wired_device.speed().await?,
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
                    _ => {}
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

enum ActiveConnectionInfo {
    Wired {
        name: String,
        hw_address: String,
        speed: u32,
    },
    WiFi {
        name: String,
        hw_address: String,
        flags: ApFlags,
        rsn_flags: ApSecurityFlags,
        wpa_flags: ApSecurityFlags,
    },
}
