use super::{NetworkManagerEvent, NetworkManagerState};
use cosmic::iced::{self, subscription};
use cosmic_dbus_networkmanager::nm::NetworkManager;
use futures::{SinkExt, StreamExt};
use log::error;
use std::fmt::Debug;
use std::hash::Hash;
use zbus::Connection;

pub fn active_conns_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
    conn: Connection,
) -> iced::Subscription<NetworkManagerEvent> {
    let initial = State::Continue(conn);
    subscription::channel(id, 50, move |mut output| {
        let mut state = initial;

        async move {
            loop {
                state = start_listening(state, &mut output).await;
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

    let mut active_conns_changed = network_manager.receive_active_connections_changed().await;
    active_conns_changed.next().await;

    while let (Some(_change), _) = tokio::join!(
        active_conns_changed.next(),
        tokio::time::sleep(tokio::time::Duration::from_secs(1))
    ) {
        let new_state = NetworkManagerState::new(&conn).await.unwrap_or_default();
        _ = output
            .send(NetworkManagerEvent::ActiveConns(new_state))
            .await;
    }

    State::Continue(conn)
}
