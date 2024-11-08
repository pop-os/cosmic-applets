// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic_dbus_networkmanager::{device::wireless::WirelessDevice, interface::enums::DeviceState};

use futures_util::StreamExt;
use itertools::Itertools;
use std::collections::HashMap;
use zbus::zvariant::ObjectPath;

pub async fn handle_wireless_device(
    device: WirelessDevice<'_>,
    hw_address: Option<String>,
) -> zbus::Result<Vec<AccessPoint>> {
    device.request_scan(HashMap::new()).await?;
    let mut scan_changed = device.receive_last_scan_changed().await;
    if let Some(t) = scan_changed.next().await {
        if let Ok(-1) = t.get().await {
            eprintln!("scan errored");
            return Ok(Default::default());
        }
    }
    let access_points = device.get_access_points().await?;
    let state: DeviceState = device
        .upcast()
        .await
        .and_then(|dev| dev.cached_state())
        .unwrap_or_default()
        .map(|s| s.into())
        .unwrap_or_else(|| DeviceState::Unknown);
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
        aps.insert(
            ssid.clone(),
            AccessPoint {
                ssid,
                strength,
                state,
                working: false,
                path: ap.inner().path().to_owned(),
                hw_address: hw_address.as_ref().unwrap_or(&"".to_string()).clone(),
            },
        );
    }
    let aps = aps
        .into_values()
        .sorted_by(|a, b| b.strength.cmp(&a.strength))
        .collect();
    Ok(aps)
}

#[derive(Debug, Clone)]
pub struct AccessPoint {
    pub ssid: String,
    pub strength: u8,
    pub state: DeviceState,
    pub working: bool,
    pub path: ObjectPath<'static>,
    pub hw_address: String,
}
