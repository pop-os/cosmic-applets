use super::{NetworkManagerEvent, NetworkManagerState};
use cosmic::iced::{self, subscription};
use cosmic_dbus_networkmanager::nm::NetworkManager;
use futures::{SinkExt, StreamExt};
use log::error;
use std::fmt::Debug;
use std::hash::Hash;
use zbus::Connection;

pub fn devices_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
    has_popup: bool,
    conn: Connection,
) -> iced::Subscription<NetworkManagerEvent> {
    let initial = State::Continue(conn.clone());
    subscription::channel((id, has_popup), 50, move |mut output| {
        let mut state = initial.clone();

        async move {
            loop {
                state = start_listening(state, has_popup, &mut output).await;
            }
        }
    })
}

#[derive(Debug, Clone)]
pub enum State {
    Continue(Connection),
    Error,
}

async fn start_listening(
    state: State,
    has_popup: bool,
    output: &mut futures::channel::mpsc::Sender<NetworkManagerEvent>,
) -> State {
    let conn = match state {
        State::Continue(conn) => conn,
        State::Error => iced::futures::future::pending().await,
    };
    let network_manager = match NetworkManager::new(&conn).await {
        Ok(n) => n,
        Err(e) => {
            error!("Failed to connect to NetworkManager: {}", e);
            return State::Error;
        }
    };

    let mut devices_changed = network_manager.receive_devices_changed().await;

    let secs = if has_popup { 4 } else { 60 };
    while let (Some(_change), _) = tokio::join!(
        devices_changed.next(),
        tokio::time::sleep(tokio::time::Duration::from_secs(secs))
    ) {
        let new_state = NetworkManagerState::new(&conn).await.unwrap_or_default();
        _ = output
            .send(NetworkManagerEvent::WirelessAccessPoints(new_state))
            .await;
    }

    State::Continue(conn)
}
