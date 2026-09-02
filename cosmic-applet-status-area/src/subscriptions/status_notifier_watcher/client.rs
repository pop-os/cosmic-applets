// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use futures::{Stream, StreamExt};
use std::pin::Pin;

use super::Event;
use crate::subscriptions::status_notifier_item::StatusNotifierItem;

// TODO: Don't use trait object
pub type EventStream = Pin<Box<dyn Stream<Item = Event> + Send>>;

#[zbus::proxy(
    interface = "org.kde.StatusNotifierWatcher",
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher"
)]
trait StatusNotifierWatcher {
    fn register_status_notifier_host(&self, name: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> zbus::Result<Vec<String>>;

    #[zbus(signal)]
    fn status_notifier_item_registered(&self, name: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    fn status_notifier_item_unregistered(&self, name: &str) -> zbus::Result<()>;
}

pub async fn watch(connection: &zbus::Connection) -> zbus::Result<EventStream> {
    let watcher = StatusNotifierWatcherProxy::new(connection).await?;

    let name = connection.unique_name().unwrap().as_str();
    if let Err(err) = watcher.register_status_notifier_host(name).await {
        eprintln!("Failed to register status notifier host: {err}");
    }

    let connection_clone = connection.clone();
    let registered_stream = watcher
        .receive_status_notifier_item_registered()
        .await?
        .then(move |evt| Box::pin(item_registered(connection_clone.clone(), evt)));
    let unregistered_stream = watcher
        .receive_status_notifier_item_unregistered()
        .await?
        .map(|evt| match evt.args() {
            Ok(args) => Event::Unregistered(args.name.to_string()),
            Err(err) => Event::Error(err.to_string()),
        });

    let items = watcher.registered_status_notifier_items().await?;
    let connection = connection.clone();
    // Seed concurrently (bounded); `buffered` keeps the watcher's enumeration order.
    let items_stream = futures::stream::iter(items.into_iter())
        .map(move |name| status_notifier_item(connection.clone(), name))
        .buffered(16);
    // Merge seed with live streams so live registrations flow during seeding.
    Ok(Box::pin(futures::stream_select!(
        items_stream,
        registered_stream,
        unregistered_stream
    )))
}

async fn item_registered(connection: zbus::Connection, evt: StatusNotifierItemRegistered) -> Event {
    match evt.args() {
        Ok(args) => status_notifier_item(connection, args.name.to_string()).await,
        Err(err) => Event::Error(err.to_string()),
    }
}

async fn status_notifier_item(connection: zbus::Connection, name: String) -> Event {
    // Cap construction; zbus' default method timeout is unbounded.
    let build = StatusNotifierItem::new(&connection, name.clone());
    match tokio::time::timeout(std::time::Duration::from_secs(5), build).await {
        Ok(Ok(item)) => Event::Registered(item),
        Ok(Err(err)) => Event::Error(err.to_string()),
        Err(_) => Event::Error(format!(
            "status notifier item `{name}` timed out during construction"
        )),
    }
}
