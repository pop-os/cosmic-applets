use super::{NetworkManagerEvent, NetworkManagerState};
use cosmic::iced::{self, subscription};
use cosmic_dbus_networkmanager::nm::NetworkManager;
use log::error;
use std::fmt::Debug;
use std::hash::Hash;
use zbus::Connection;
use futures::StreamExt;

pub fn devices_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
    conn: Connection,
) -> iced::Subscription<(I, NetworkManagerEvent)> {
    subscription::unfold(id, State::Continue(conn), move |mut state| async move {
        loop {
            let (update, new_state) = start_listening(id, state).await;
            state = new_state;
            if let Some(update) = update {
                return (update, state);
            }
        }
    })
}

#[derive(Debug, Clone)]
pub enum State {
    Continue(Connection),
    Error,
}

async fn start_listening<I: Copy + Debug>(
    id: I,
    state: State,
) -> (Option<(I, NetworkManagerEvent)>, State) {
    let conn = match state {
        State::Continue(conn) => conn,
        State::Error => iced::futures::future::pending().await,
    };
    let network_manager = match NetworkManager::new(&conn).await {
        Ok(n) => n,
        Err(e) => {
            error!("Failed to connect to NetworkManager: {}", e);
            return (None, State::Error);
        }
    };

    let mut devices_changed = network_manager.receive_devices_changed().await;
    devices_changed.next().await;

    let new_state = NetworkManagerState::new(&conn).await.unwrap_or_default();

    (
        Some((
            id,
            NetworkManagerEvent::WirelessAccessPoints(new_state),
        )),
        State::Continue(conn),
    )
}
