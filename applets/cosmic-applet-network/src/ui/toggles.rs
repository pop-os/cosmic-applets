use crate::{task, widgets::SettingsEntry};
use cosmic_dbus_networkmanager::nm::NetworkManager;
use futures_util::StreamExt;
use gtk4::{
    glib::{self, clone, source::PRIORITY_DEFAULT, MainContext, Sender},
    prelude::*,
    Inhibit, Orientation, Separator, Switch,
};
use zbus::Connection;

pub fn add_toggles(target: &gtk4::Box) {
    view! {
        airplane_mode = SettingsEntry {
            set_title_markup: "<b>Airplane Mode</b>",
            set_child: airplane_mode_switch = &Switch {}
        }
    }
    view! {
        wifi = SettingsEntry {
            set_title_markup: "<b>WiFi</b>",
            set_child: wifi_switch = &Switch {}
        }
    }
    target.append(&airplane_mode);
    target.append(&Separator::new(Orientation::Horizontal));
    target.append(&wifi);
    target.append(&Separator::new(Orientation::Horizontal));

    let (wifi_tx, wifi_rx) = MainContext::channel::<bool>(PRIORITY_DEFAULT);
    wifi_switch.connect_state_set(
        clone!(@strong wifi_tx => @default-return Inhibit(false),  move |_switch, state| {
            match task::block_on(set_wifi_mode(state)) {
                Ok(()) => Inhibit(false),
                Err(err) => {
                    eprintln!("set_wifi_mode failed: {}", err);
                    Inhibit(true)
                }
            }
        }),
    );
    wifi_rx.attach(
        None,
        clone!(@weak wifi_switch => @default-return Continue(true), move |wifi| {
            wifi_switch.set_active(wifi);
            Continue(true)
        }),
    );
    task::spawn(get_wifi_mode(wifi_tx));
}

async fn get_wifi_mode(tx: Sender<bool>) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let network_manager = NetworkManager::new(&connection).await?;
    let wireless_enabled = network_manager.wireless_enabled().await?;
    tx.send(wireless_enabled)
        .expect("Failed to send wifi enablement back to main thread");
    tokio::spawn(async move {
        let connection = Connection::system().await?;
        let network_manager = NetworkManager::new(&connection).await?;
        let mut stream = network_manager.receive_wireless_enabled_changed().await;
        while let Some(wireless_enabled) = stream.next().await {
            if let Ok(wireless_enabled) = wireless_enabled.get().await {
                tx.send(wireless_enabled)
                    .expect("Failed to send wifi enablement back to main thread");
            }
        }
        zbus::Result::Ok(())
    });
    Ok(())
}

async fn set_wifi_mode(state: bool) -> zbus::Result<()> {
    let connection = Connection::system().await?;
    let network_manager = NetworkManager::new(&connection).await?;
    network_manager.set_wireless_enabled(state).await
}
