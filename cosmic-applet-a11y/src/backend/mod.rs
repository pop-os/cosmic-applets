// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::futures::FutureExt;
use cosmic::{
    iced::{
        self,
        futures::{self, select, SinkExt, StreamExt},
        Subscription,
    },
    iced_futures::stream,
};
use cosmic_dbus_a11y::*;
use std::{fmt::Debug, hash::Hash};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use zbus::{Connection, Result};

#[derive(Debug, Clone)]
pub enum Update {
    Error(String),
    Status(bool),
    Init(bool, UnboundedSender<A11yRequest>),
}

pub enum A11yRequest {
    Status(bool),
}

#[derive(Debug)]
pub enum State {
    Ready,
    Waiting(Connection, u8, bool, UnboundedReceiver<A11yRequest>),
    Finished,
}

pub fn subscription() -> iced::Subscription<Update> {
    struct MyId;

    Subscription::run_with_id(
        std::any::TypeId::of::<MyId>(),
        stream::channel(50, move |mut output| async move {
            let mut state = State::Ready;

            loop {
                state = start_listening(state, &mut output).await;
            }
        }),
    )
}

async fn start_listening(
    state: State,
    output: &mut futures::channel::mpsc::Sender<Update>,
) -> State {
    match state {
        State::Ready => {
            let conn = match Connection::session().await.map_err(|e| e.to_string()) {
                Ok(conn) => conn,
                Err(e) => {
                    _ = output.send(Update::Error(e)).await;
                    return State::Finished;
                }
            };
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let mut enabled = false;
            if let Ok(proxy) = StatusProxy::new(&conn).await {
                if let Ok(status) = proxy.screen_reader_enabled().await {
                    enabled = status;
                }
            }
            _ = output.send(Update::Init(enabled, tx)).await;
            State::Waiting(conn, 20, enabled, rx)
        }
        State::Waiting(conn, mut retry, mut enabled, mut rx) => {
            let Ok(proxy) = StatusProxy::new(&conn).await else {
                if retry == 0 {
                    tracing::error!("Accessibility Status is unavailable.");
                    return State::Finished;
                } else {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(
                        2_u64.pow(retry as u32),
                    ))
                    .await;
                    retry -= 1;
                    return State::Waiting(conn, retry, enabled, rx);
                }
            };
            retry = 20;

            let mut watch_changes = proxy.receive_screen_reader_enabled_changed().await;

            if let Ok(status) = proxy.screen_reader_enabled().await {
                if enabled != status {
                    _ = output.send(Update::Status(enabled));
                }
                enabled = status;
            }

            loop {
                if let Ok(status) = proxy.screen_reader_enabled().await {
                    if enabled != status {
                        _ = output.send(Update::Status(enabled));
                    }
                    enabled = status;
                }

                let mut next_change = Box::pin(watch_changes.next()).fuse();
                let mut next_request = Box::pin(rx.recv()).fuse();

                select! {
                    v = next_request => {
                        match v {
                            Some(A11yRequest::Status(is_enabled)) => {
                                // Set status
                                enabled = is_enabled;
                                _ = proxy.set_is_enabled(is_enabled).await;
                                _ = proxy.set_screen_reader_enabled(is_enabled).await;
                            }
                            None => return State::Finished,
                        }
                    }
                    v = next_change => {
                        match v {
                            Some(f) => {
                                if let Ok(enabled) = f.get().await {
                                    _ = output.send(Update::Status(enabled));
                                }
                            }
                            None => break,
                        };
                    }
                }
            }

            State::Waiting(conn, retry, enabled, rx)
        }
        State::Finished => iced::futures::future::pending().await,
    }
}
