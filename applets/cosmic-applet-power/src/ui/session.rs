// SPDX-License-Identifier: LGPL-3.0-or-later

use crate::{session_manager::SessionManagerProxy, RT};
use gtk4::{prelude::*, Align, Button, Image, Label, Orientation};
use logind_zbus::{
    manager::ManagerProxy,
    session::{SessionProxy, SessionType},
    user::UserProxy,
};
use nix::unistd::getuid;
use zbus::Connection;

async fn lock_screen() -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let manager_proxy = ManagerProxy::new(&connection).await?;
    // Get the session this current process is running in
    let our_uid = getuid().as_raw() as u32;
    let user_path = manager_proxy.get_user(our_uid).await?;
    let user = UserProxy::builder(&connection)
        .path(user_path)?
        .build()
        .await?;
    // Lock all non-TTY sessions of this user
    let sessions = user.sessions().await?;
    for (_, session_path) in sessions {
        let session = SessionProxy::builder(&connection)
            .path(session_path)?
            .build()
            .await?;
        if session.type_().await? != SessionType::TTY {
            session.lock().await?;
        }
    }
    Ok(())
}

async fn log_out() -> zbus::Result<()> {
    let connection = Connection::session().await?;
    let manager_proxy = SessionManagerProxy::new(&connection).await?;
    manager_proxy.logout(0).await
}

pub fn build() -> gtk4::Box {
    view! {
        inner_box = gtk4::Box {
            set_orientation: Orientation::Vertical,
            set_spacing: 5,
            append: lock_screen_button = &Button {
                set_child: lock_screen_box = Some(&gtk4::Box) {
                    set_orientation: Orientation::Horizontal,
                    set_spacing: 10,
                    append: lock_screen_icon = &Image {
                        set_icon_name: Some("system-lock-screen-symbolic"),
                    },
                    append: lock_screen_label = &Label {
                        set_label: "Lock Screen",
                        set_halign: Align::Start,
                        set_hexpand: true
                    },
                    append: lock_screen_hotkey_label = &Label {
                        set_label: "Super + Escape",
                        set_halign: Align::End
                    }
                }
            },
            append: log_out_button = &Button {
                set_child: log_out_box = Some(&gtk4::Box) {
                    set_orientation: Orientation::Horizontal,
                    set_spacing: 10,
                    append: log_out_icon = &Image {
                        set_icon_name: Some("system-log-out-symbolic"),
                    },
                    append: log_out_label = &Label {
                        set_label: "Log Out",
                        set_halign: Align::Start,
                        set_hexpand: true
                    },
                    append: log_out_hotkey_label = &Label {
                        set_label: "Ctrl + Alt + Delete",
                        set_halign: Align::End
                    }
                }
            }
        }
    }
    lock_screen_button.connect_clicked(|_| {
        RT.spawn(async move {
            lock_screen().await.expect("failed to lock screen");
        });
    });
    log_out_button.connect_clicked(|_| {
        RT.spawn(async move {
            log_out().await.expect("failed to log out");
        });
    });
    inner_box
}
