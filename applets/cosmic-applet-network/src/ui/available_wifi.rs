// SPDX-License-Identifier: LGPL-3.0-or-later

use cosmic_dbus_networkmanager::{
    device::SpecificDevice, interface::enums::ApSecurityFlags, nm::NetworkManager,
};
use futures_util::{StreamExt, TryFutureExt};
use gtk4::{
    glib, prelude::*, Align, Button, Dialog, HeaderBar, Image, Label, Orientation, ScrolledWindow,
    Spinner,
};
use itertools::Itertools;
use slotmap::{DefaultKey, SlotMap};
use std::{
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::mpsc::UnboundedSender;
use zbus::Connection;

pub fn add_available_wifi(target: &gtk4::Box) {}

async fn scan_for_devices(tx: UnboundedSender<SlotMap<DefaultKey, AccessPoint>>) {
    let sys_conn = match Connection::system().await {
        Ok(conn) => conn,
        Err(err) => {
            //error!(%err, "Failed to connect to system dbus session");
            return;
        }
    };
    let nm = match NetworkManager::new(&sys_conn).await {
        Ok(p) => p,
        Err(err) => {
            //error!(%err, "Failed to set up connection to NetworkManager dbus");
            return;
        }
    };
    let devices = match nm.devices().await {
        Ok(d) => d,
        Err(err) => {
            //error!(%err, "Failed to get devices from NetworkManager");
            return;
        }
    };
    let mut all_aps = SlotMap::new();

    for d in devices {
        if let Ok(Some(SpecificDevice::Wireless(w))) = d.downcast_to_device().await {
            let id = d
                .active_connection()
                .and_then(|ac| async move { ac.id().await })
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            if let Err(err) = w.request_scan(std::collections::HashMap::new()).await {
                //error!(%err, %id, "Wi-Fi scan failed");
                continue;
            };
            let mut scan_changed = w.receive_last_scan_changed().await;
            if let Some(t) = scan_changed.next().await {
                if let Ok(t) = t.get().await {
                    if t == -1 {
                        //error!(%id, "Getting access point failed");
                        continue;
                    }
                }
                match w.get_access_points().await {
                    Ok(aps) => {
                        if !aps.is_empty() {
                            for ap in AccessPoint::from_list(aps).await {
                                all_aps.insert(ap);
                            }

                            break;
                        }
                    }
                    Err(err) => {
                        //error!(%err, %id, "Getting access points failed");
                        continue;
                    }
                };
            }
        }
    }

    if let Err(err) = tx.send(all_aps) {
        //error!(%err, "failed to send AP list");
    }
}

#[derive(Debug)]
pub struct AccessPoint {
    pub ssid: String,
    pub hw_address: String,
    pub strength: u8,
    pub wpa_flags: ApSecurityFlags,
}

impl AccessPoint {
    pub async fn new(
        ap: cosmic_dbus_networkmanager::access_point::AccessPoint<'_>,
    ) -> Option<Self> {
        Some(Self {
            ssid: ap
                .ssid()
                .await
                .map(|x| String::from_utf8_lossy(&x).into_owned())
                .ok()?,
            hw_address: ap.hw_address().await.ok()?,
            strength: ap.strength().await.ok()?,
            wpa_flags: ap.wpa_flags().await.ok()?,
        })
    }

    pub async fn from_list(
        aps: Vec<cosmic_dbus_networkmanager::access_point::AccessPoint<'_>>,
    ) -> Vec<Self> {
        let mut out = Vec::<Self>::with_capacity(aps.len());
        for ap in aps {
            if let Some(ap) = Self::new(ap).await {
                out.push(ap);
            }
        }
        let mut ret = out
            .into_iter()
            .sorted_by(|a, b| a.strength.cmp(&b.strength))
            .rev()
            .unique_by(|ap| ap.ssid.clone())
            .collect::<Vec<Self>>();
        // for some reason adding .rev() messes up unique_by, so we do this instead
        ret.reverse();
        ret
    }
}
