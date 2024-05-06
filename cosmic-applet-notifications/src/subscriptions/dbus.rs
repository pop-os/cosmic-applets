// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::subscriptions::freedesktop_proxy::NotificationsProxy;
use cosmic::{
    iced::{
        futures::{self, SinkExt},
        subscription,
    },
    iced_futures::Subscription,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::{error, warn};
use zbus::{export::futures_util::StreamExt, Connection};

#[derive(Debug)]
pub enum State {
    Ready,
    WaitingForNotificationEvent(NotificationsProxy<'static>, Receiver<Input>),
    Finished,
}

#[derive(Debug, Clone, Copy)]
pub enum Input {
    Dismiss(u32),
    CloseEvent(u32),
}

#[derive(Debug, Clone)]
pub enum Output {
    Ready(Sender<Input>),
    CloseEvent(u32),
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

                        let Ok(proxy) = NotificationsProxy::new(&conn).await else {
                            error!("Failed to create proxy from session connection");
                            state = State::Finished;
                            continue;
                        };
                        let tx = sender.clone();
                        if let Err(err) = output.send(Output::Ready(sender)).await {
                            error!("Failed to send sender: {}", err);
                            state = State::Finished;
                            continue;
                        }
                        state = match proxy.receive_notification_closed().await {
                            Ok(mut s) => {
                                tokio::spawn(async move {
                                    while let Some(msg) = s.next().await {
                                        let Ok(id) = msg.args().map(|args| args.id) else {
                                            continue;
                                        };
                                        _ = tx.send(Input::CloseEvent(id)).await;
                                    }
                                });
                                State::WaitingForNotificationEvent(proxy, receiver)
                            }
                            Err(err) => {
                                error!(
                                    "failed to get a stream of signals for notifications. {}",
                                    err
                                );
                                State::Finished
                            }
                        };
                    }
                    State::WaitingForNotificationEvent(proxy, rx) => match rx.recv().await {
                        Some(Input::Dismiss(id)) => {
                            if let Err(err) = proxy.close_notification(id).await {
                                error!("Failed to close notification: {}", err);
                            }
                        }
                        Some(Input::CloseEvent(id)) => {
                            _ = output.send(Output::CloseEvent(id)).await;
                        }
                        None => {
                            warn!("Notification event channel closed");
                            state = State::Finished;
                            continue;
                        }
                    },
                    State::Finished => {
                        let () = futures::future::pending().await;
                    }
                }
            }
        },
    )
}
