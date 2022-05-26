// SPDX-License-Identifier: GPL-3.0-or-later

use crate::RT;
use gtk4::{prelude::*, Button, IconSize, Image, Label, Orientation};
use logind_zbus::manager::ManagerProxy;
use zbus::Connection;

async fn restart() -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    manager_proxy.reboot(true).await
}

async fn shut_down() -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    manager_proxy.power_off(true).await
}

async fn suspend() -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    manager_proxy.suspend(true).await
}

pub fn build() -> gtk4::Box {
    let suspend_button = create_button("Suspend", "system-suspend-symbolic");
    let restart_button = create_button("Restart", "system-reboot-symbolic");
    let shut_down_button = create_button("Shut Down", "system-shutdown-symbolic");
    suspend_button.connect_clicked(|_| {
        RT.spawn(async move {
            suspend().await.expect("failed to suspend system");
        });
    });
    restart_button.connect_clicked(|_| {
        RT.spawn(async move {
            restart().await.expect("failed to reboot system");
        });
    });
    shut_down_button.connect_clicked(|_| {
        RT.spawn(async move {
            shut_down().await.expect("failed to shut down system");
        });
    });
    view! {
        inner_box = gtk4::Box {
            set_orientation: Orientation::Horizontal,
            set_spacing: 24,
            append: &suspend_button,
            append: &restart_button,
            append: &shut_down_button,
        }
    }
    inner_box
}

pub fn create_button(name: &str, icon_name: &str) -> Button {
    view! {
        button = Button {
            set_child: inner_box = Some(&gtk4::Box) {
                set_orientation: Orientation::Vertical,
                set_spacing: 8,
                set_margin_start: 8,
                set_margin_end: 8,
                set_margin_top: 8,
                set_margin_bottom: 8,
                append: icon = &Image {
                    set_icon_name: Some(icon_name),
                    set_icon_size: IconSize::Large
                },
                append: label = &Label {
                    set_label: name
                }
            }
        }
    }
    button
}
