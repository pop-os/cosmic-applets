// SPDX-License-Identifier: LGPL-3.0-or-later

use crate::task;
use cosmic_dbus_networkmanager::{
    device::{wireless::WirelessDevice, SpecificDevice},
    nm::NetworkManager,
};
use futures_util::StreamExt;
use gtk4::{
    glib::{source::PRIORITY_DEFAULT, MainContext, Sender},
    prelude::*,
};
use tokio::sync::mpsc::UnboundedSender;
use zbus::Connection;

pub fn add_available_wifi(target: &gtk4::Box) {
    let (tx, rx) = MainContext::channel::<Vec<AccessPoint>>(PRIORITY_DEFAULT);
    task::spawn(scan_for_wifi(tx));
    rx.attach(None, |aps| Continue(true));
}

async fn scan_for_wifi(tx: Sender<Vec<AccessPoint>>) -> zbus::Result<()> {
    let conn = Connection::system().await?;
    let network_manager = NetworkManager::new(&conn).await?;
    loop {
        let devices = network_manager.devices().await?;
        for device in devices {
            if let Ok(Some(SpecificDevice::Wireless(wireless_device))) =
                device.downcast_to_device().await
            {
                handle_wireless_device(wireless_device, tx.clone()).await?;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn handle_wireless_device(
    device: WirelessDevice<'_>,
    tx: Sender<Vec<AccessPoint>>,
) -> zbus::Result<()> {
    device
        .request_scan(std::collections::HashMap::new())
        .await?;
    let mut scan_changed = device.receive_last_scan_changed().await;
    if let Some(t) = scan_changed.next().await {
        if let Ok(-1) = t.get().await {
            return Ok(());
        }
    }
    let access_points = device.get_access_points().await?;
    let mut aps = Vec::with_capacity(access_points.len());
    for ap in access_points {
        aps.push(AccessPoint {
            ssid: String::from_utf8_lossy(&ap.ssid().await?.clone()).into_owned(),
            strength: ap.strength().await?,
        });
    }
    Ok(())
}

struct AccessPoint {
    ssid: String,
    strength: u8,
}
