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
    IconSize, Image, ListBox, ListBoxRow, Orientation,
};
use std::{cell::RefCell, net::IpAddr, rc::Rc};
use zbus::Connection;

pub fn add_current_networks(target: &gtk4::Box) {
    let networks_list = ListBox::builder().show_separators(true).build();
    let entries = Rc::<RefCell<Vec<ListBoxRow>>>::default();
    let (tx, rx) = MainContext::channel::<Vec<ActiveConnectionInfo>>(PRIORITY_DEFAULT);
    crate::task::spawn(handle_devices(tx));
    rx.attach(
        None,
        clone!(@weak networks_list, @strong entries => @default-return Continue(true), move |connections| {
            let mut entries = entries.borrow_mut();
            display_active_connections(connections, &networks_list, &mut *entries);
            Continue(true)
        }),
    );
    target.append(&networks_list);
}

fn display_active_connections(
    connections: Vec<ActiveConnectionInfo>,
    target: &ListBox,
    entries: &mut Vec<ListBoxRow>,
) {
    for old_entry in entries.drain(..) {
        target.remove(&old_entry);
    }
    for connection in connections {
        let entry = match connection {
            ActiveConnectionInfo::Wired {
                name,
                hw_address,
                speed,
                ip_addresses,
            } => render_wired_connection(name, speed, ip_addresses),
            ActiveConnectionInfo::WiFi {
                name,
                hw_address,
                flags,
                rsn_flags,
                wpa_flags,
            } => todo!(),
        };
        let entry = ListBoxRow::builder().child(&entry).build();
        target.append(&entry);
        entries.push(entry);
    }
}

fn render_wired_connection(name: String, speed: u32, ip_addresses: Vec<IpAddr>) -> gtk4::Box {
    view! {
        entry = gtk4::Box {
            set_orientation: Orientation::Horizontal,
            set_spacing: 8,
            append: wired_icon = &Image {
                set_icon_name: Some("network-wired-symbolic"),
                set_icon_size: IconSize::Large
            },
            append: wired_label_box = &gtk4::Box {
                set_orientation: Orientation::Vertical,
                append: wired_label = &gtk4::Label {
                    set_label: &name,
                    set_halign: gtk4::Align::Start,
                }
            },
            append: wired_speed = &gtk4::Label {
                set_label: &format!("Connected - {} Mbps", speed),
                set_valign: gtk4::Align::Center,
            },
        }
    }
    for address in ip_addresses {
        view! {
            wired_ip = gtk4::Label {
                set_label: &format!("IP Address: {}", address),
                set_halign: gtk4::Align::Start,
            }
        }
        wired_label_box.append(&wired_ip);
    }
    entry
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
                        let mut ip_addresses = Vec::new();
                        for address_data in device.ip4_config().await?.address_data().await? {
                            ip_addresses.push(IpAddr::V4(address_data.address));
                        }
                        for address_data in device.ip6_config().await?.address_data().await? {
                            ip_addresses.push(IpAddr::V6(address_data.address));
                        }
                        info.push(ActiveConnectionInfo::Wired {
                            name: connection.id().await?,
                            hw_address: wired_device.hw_address().await?,
                            speed: wired_device.speed().await?,
                            ip_addresses,
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
        tx.send(info)
            .expect("failed to send active connections back to main thread");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

enum ActiveConnectionInfo {
    Wired {
        name: String,
        hw_address: String,
        speed: u32,
        ip_addresses: Vec<IpAddr>,
    },
    WiFi {
        name: String,
        hw_address: String,
        flags: ApFlags,
        rsn_flags: ApSecurityFlags,
        wpa_flags: ApSecurityFlags,
    },
}
