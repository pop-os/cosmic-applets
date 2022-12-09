// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic_dbus_networkmanager::device::wireless::WirelessDevice;

use futures_util::StreamExt;
use itertools::Itertools;
use std::collections::HashMap;

pub async fn handle_wireless_device(device: WirelessDevice<'_>) -> zbus::Result<Vec<AccessPoint>> {
    device.request_scan(HashMap::new()).await?;
    let mut scan_changed = device.receive_last_scan_changed().await;
    if let Some(t) = scan_changed.next().await {
        if let Ok(-1) = t.get().await {
            eprintln!("scan errored");
            return Ok(Default::default());
        }
    }
    let access_points = device.get_access_points().await?;
    // Sort by strength and remove duplicates
    let mut aps = HashMap::<String, AccessPoint>::new();
    for ap in access_points {
        let ssid = String::from_utf8_lossy(&ap.ssid().await?.clone()).into_owned();
        let strength = ap.strength().await?;
        if let Some(access_point) = aps.get(&ssid) {
            if access_point.strength > strength {
                continue;
            }
        }
        aps.insert(ssid.clone(), AccessPoint { ssid, strength });
    }
    let aps = aps
        .into_iter()
        .map(|(_, x)| x)
        .sorted_by(|a, b| b.strength.cmp(&a.strength))
        .collect();
    Ok(aps)
}

#[derive(Debug, Clone)]
pub struct AccessPoint {
    pub ssid: String,
    pub strength: u8,
}
