// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic_dbus_networkmanager::{
    device::wireless::WirelessDevice,
    interface::{
        access_point::AccessPointProxy,
        enums::{ApFlags, ApSecurityFlags, DeviceState},
    },
};

use futures_util::StreamExt;
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use zbus::zvariant::ObjectPath;

use super::hw_address::HwAddress;

pub async fn handle_wireless_device(
    device: WirelessDevice<'_>,
    hw_address: Option<String>,
) -> zbus::Result<Vec<AccessPoint>> {
    device.request_scan(HashMap::new()).await?;
    let mut scan_changed = device.receive_last_scan_changed().await;
    if let Some(t) = scan_changed.next().await {
        if let Ok(-1) = t.get().await {
            eprintln!("scan errored");
            return Ok(Vec::new());
        }
    }
    let access_points = device.get_access_points().await?;
    let state: DeviceState = device
        .upcast()
        .await
        .and_then(|dev| dev.cached_state())
        .unwrap_or_default()
        .map_or(DeviceState::Unknown, std::convert::Into::into);
    // Sort by strength and remove duplicates
    let mut aps = FxHashMap::<String, AccessPoint>::default();
    for ap in access_points {
        let ssid = String::from_utf8_lossy(ap.ssid().await?.as_slice()).into_owned();
        let wps_push = ap.flags().await?.contains(ApFlags::WPS_PBC);
        let strength = ap.strength().await?;
        if let Some(access_point) = aps.get(&ssid) {
            if access_point.strength > strength {
                continue;
            }
        }
        let proxy: &AccessPointProxy = &ap;
        let Ok(flags) = ap.rsn_flags().await else {
            continue;
        };

        let network_type = if flags.intersects(ApSecurityFlags::KEY_MGMT_802_1X) {
            NetworkType::EAP
        } else if flags.intersects(ApSecurityFlags::KEY_MGMTPSK) {
            NetworkType::PSK
        } else if flags.is_empty() {
            NetworkType::Open
        } else {
            continue;
        };

        aps.insert(
            ssid.clone(),
            AccessPoint {
                ssid,
                strength,
                state,
                working: false,
                path: ap.inner().path().to_owned(),
                hw_address: hw_address
                    .as_ref()
                    .and_then(|str_addr| HwAddress::from_str(str_addr))
                    .unwrap_or_default(),
                wps_push,
                network_type,
            },
        );
    }
    let mut aps = aps.into_values().collect::<Vec<_>>();
    aps.sort_unstable_by_key(|ap| ap.strength);
    Ok(aps)
}

#[derive(Debug, Clone)]
pub struct AccessPoint {
    pub ssid: String,
    pub strength: u8,
    pub state: DeviceState,
    pub working: bool,
    pub path: ObjectPath<'static>,
    pub hw_address: HwAddress,
    pub wps_push: bool,
    pub network_type: NetworkType,
}

// TODO do we want to support eap methods other than peap in the applet?
// Then we'd need a dropdown for the eap method,
// and tls requires a cert instead of a password
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy)]
pub enum NetworkType {
    Open,
    PSK,
    EAP,
}
