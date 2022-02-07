// SPDX-License-Identifier: LGPL-3.0-or-later

use cosmic_dbus_networkmanager::{
    access_point::AccessPoint, device::wired::WiredDevice, nm::NetworkManager,
};
use gtk4::glib::{self, clone, source::PRIORITY_DEFAULT, MainContext, Sender};
use zbus::Connection;

pub fn add_current_networks(target: &gtk4::Box) {
    crate::task::spawn(handle_devices());
}

fn add_vpn(target: &gtk4::Box) {}

fn add_access_point(target: &gtk4::Box, access_point: &AccessPoint) {}

fn add_wired_device(target: &gtk4::Box, device: &WiredDevice) {}

async fn handle_devices() -> zbus::Result<()> {
    let conn = Connection::system().await?;
    let network_manager = NetworkManager::new(&conn).await?;
    loop {
        let active_connections = network_manager.active_connections().await?;
        for connection in active_connections {
            eprintln!("{}", connection.type_().await?);
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
