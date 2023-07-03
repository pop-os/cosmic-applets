use crate::subscriptions::dbus_proxy::NotificationsProxy;
use cosmic::{
    iced::{
        futures::{self, SinkExt},
        subscription,
    },
    iced_futures::Subscription,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::{error, warn};
use zbus::Connection;

#[derive(Debug)]
pub enum State {
    Ready,
    WaitingForNotificationEvent(Connection, Receiver<Input>),
    Finished,
}

#[derive(Debug, Clone, Copy)]
pub enum Input {
    Dismiss(u32),
}

#[derive(Debug, Clone)]
pub enum Output {
    Ready(Sender<Input>),
}

pub fn proxy() -> Subscription<Output> {
    struct SomeWorker;

    subscription::channel(
        std::any::TypeId::of::<SomeWorker>(),
        50,
        |mut output| async move {
            let mut state = State::Ready;

            loop {
                match &mut state {
                    State::Ready => {
                        let (sender, receiver) = channel(10);
                        let Ok(conn) = Connection::session().await else {
                            error!("Failed to connect to session bus");
                            state = State::Finished;
                            continue;
                        };
                        if let Err(err) = output.send(Output::Ready(sender)).await {
                            error!("Failed to send sender: {}", err);
                            state = State::Finished;
                            continue;
                        }

                        state = State::WaitingForNotificationEvent(conn, receiver);
                    }
                    State::WaitingForNotificationEvent(conn, rx) => {
                        let Ok(proxy) = NotificationsProxy::new(&conn).await else {
                            error!("Failed to create proxy from session connection");
                            state = State::Finished;
                            continue;
                        };

                        match rx.recv().await {
                            Some(Input::Dismiss(id)) => {
                                if let Err(err) = proxy.close_notification(id).await {
                                    error!("Failed to close notification: {}", err);
                                }
                            }
                            None => {
                                warn!("Notification event channel closed");
                                state = State::Finished;
                                continue;
                            }
                        }
                    }
                    State::Finished => {
                        let () = futures::future::pending().await;
                    }
                }
            }
        },
    )
}
