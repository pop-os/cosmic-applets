// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

// TODO: Both this and server proxy could emit same events, have way to generate stream from either?

use cosmic::iced::{self, Subscription};
use futures::{stream, StreamExt};

use crate::subscriptions::status_notifier_item::StatusNotifierItem;

mod client;
mod server;

#[derive(Clone, Debug)]
pub enum Event {
    Connected(zbus::Connection),
    Registered(StatusNotifierItem),
    Unregistered(String),
    Error(String), // XXX
}

enum State {
    NotConnected,
    Connected(client::EventStream),
    Failed,
}

pub fn subscription() -> iced::Subscription<Event> {
    Subscription::run_with_id(
        "status-notifier-watcher",
        stream::unfold(State::NotConnected, |state| async move {
            match state {
                State::NotConnected => match connect().await {
                    Ok((connection, stream)) => {
                        Some((Event::Connected(connection), State::Connected(stream)))
                    }
                    Err(err) => Some((Event::Error(err.to_string()), State::Failed)),
                },
                State::Connected(mut stream) => match stream.next().await {
                    Some(event) => Some((event, State::Connected(stream))),
                    None => None,
                },
                State::Failed => None,
            }
        }),
    )
}

async fn connect() -> zbus::Result<(zbus::Connection, client::EventStream)> {
    // Connect to session dbus socket
    let connection = zbus::Connection::session().await?;

    // Start `StatusNotifierWatcher` service, if there isn't one running already
    server::create_service(&connection).await?;

    // Connect client and listen for registered/unregistered
    let stream = client::watch(&connection).await?;

    Ok((connection, stream))
}
