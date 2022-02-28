// SPDX-License-Identifier: LGPL-3.0-or-later

use crate::task;
use cosmic_dbus_networkmanager::{
    device::{wireless::WirelessDevice, SpecificDevice},
    nm::NetworkManager,
};
use futures_util::StreamExt;
use gtk4::{
    glib::{self, clone, source::PRIORITY_DEFAULT, MainContext, Sender},
    prelude::*,
    Image, ListBox, ListBoxRow, Separator,
};
use libcosmic_widgets::{relm4::RelmContainerExt, LabeledItem};
use std::{cell::RefCell, rc::Rc};
use zbus::Connection;

pub fn add_available_wifi(target: &gtk4::Box, separator: Separator) {
    let ap_entries = Rc::<RefCell<Vec<ListBoxRow>>>::default();
    let (tx, rx) = MainContext::channel::<Vec<AccessPoint>>(PRIORITY_DEFAULT);
    task::spawn(async move {
        if let Err(err) = scan_for_wifi(tx).await {
            eprintln!("scan_for_wifi failed: {}", err);
        }
    });
    let wifi_list = ListBox::new();
    rx.attach(
        None,
        clone!(@strong ap_entries, @weak wifi_list, @weak separator, => @default-return Continue(true), move |aps| {
            build_aps_list(ap_entries.clone(), &wifi_list, aps);
            separator.set_visible(!ap_entries.borrow().is_empty());
            Continue(true)
        }),
    );
    target.append(&wifi_list);
}

fn build_aps_list(
    ap_entries: Rc<RefCell<Vec<ListBoxRow>>>,
    target: &ListBox,
    aps: Vec<AccessPoint>,
) {
    let mut ap_entries = ap_entries.borrow_mut();
    for old_ap_box in ap_entries.drain(..) {
        target.remove(&old_ap_box);
    }
    for ap in aps {
        view! {
            entry = ListBoxRow {
                set_child: entry_box = Some(&gtk4::Box) {
                    container_add: labeled_item = &LabeledItem {
                        set_title: &ap.ssid,
                        set_child: icon = &Image {
                            set_icon_name: Some("network-wireless-symbolic")
                        }
                    }
                }
            }
        }
        target.append(&entry);
        ap_entries.push(entry);
    }
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
            eprintln!("scan errored");
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
    tx.send(aps).expect("failed to send back to main thread");
    Ok(())
}

#[derive(Debug)]
struct AccessPoint {
    ssid: String,
    strength: u8,
}
