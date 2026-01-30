// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

// TODO Doc

use crate::subscriptions::status_notifier_watcher::server::create_service;
use crate::unique_names::UniqueNames;

use futures::StreamExt;
use std::collections::HashSet;
use zbus::fdo;
use zbus::message::Header;

const DBUS_NAME: &str = "com.system76.CosmicStatusNotifierWatcher";
const OBJECT_PATH: &str = "/CosmicStatusNotifierWatcher";

/// Run daemon
pub fn run() -> cosmic::iced::Result {
    if let Err(err) = run_inner() {
        eprintln!("Zbus error running status notifier watcher: {}", err);
        std::process::exit(1);
    }
    Ok(())
}

/// Register client with daemon
pub async fn cosmic_register(conn: &zbus::Connection) -> zbus::Result<()> {
    let cosmic_watcher = CosmicAppletStatusNotifierWatcherProxy::new(conn).await?;
    cosmic_watcher.register_applet().await?;
    let mut stream = cosmic_watcher.0.receive_owner_changed().await?;
    tokio::spawn(async move {
        while let Some(value) = stream.next().await {
            if let Some(_unique_name) = value {
                /// Register with new owner
                let _ = cosmic_watcher.register_applet().await;
            }
        }
    });
    Ok(())
}

#[zbus::proxy(
    interface = "com.system76.CosmicStatusNotifierWatcher",
    default_service = "com.system76.CosmicStatusNotifierWatcher",
    default_path = "/CosmicStatusNotifierWatcher"
)]
trait CosmicAppletStatusNotifierWatcher {
    async fn register_applet(&self) -> zbus::Result<()>;
}

struct CosmicAppletStatusNotifierWatcher {
    applets: HashSet<zbus::names::UniqueName<'static>>,
    unique_names: UniqueNames,
}

#[zbus::interface(name = "com.system76.CosmicStatusNotifierWatcher")]
impl CosmicAppletStatusNotifierWatcher {
    fn register_applet(&mut self, #[zbus(header)] hdr: Header<'_>) {
        if let Some(sender) = hdr.sender() {
            if self.unique_names.has_unique_name(sender) {
                self.applets.insert(sender.to_owned());
            }
        }
    }
}

impl CosmicAppletStatusNotifierWatcher {
    fn has_client(&self) -> bool {
        !self.applets.is_empty()
    }

    /// Purge registered clients that no longer exist on bus
    fn refresh(&mut self) {
        self.applets
            .retain(|n| self.unique_names.has_unique_name(n));
    }
}

#[tokio::main]
pub async fn run_inner() -> zbus::Result<()> {
    let (running, abort_handle) = futures::future::abortable(std::future::pending::<()>());

    let conn = zbus::Connection::session().await?;
    create_service(&conn).await?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
    conn.object_server()
        .at(
            OBJECT_PATH,
            CosmicAppletStatusNotifierWatcher {
                applets: HashSet::new(),
                unique_names: UniqueNames::new(&conn).await?,
            },
        )
        .await?;
    let interface = conn
        .object_server()
        .interface::<_, CosmicAppletStatusNotifierWatcher>(OBJECT_PATH)
        .await?;
    tokio::spawn(refresh_task(interface.clone(), abort_handle.clone()));
    let name_lost_stream = dbus.receive_name_lost().await?;
    tokio::spawn(name_lost_task(name_lost_stream, abort_handle));
    conn.request_name(DBUS_NAME).await?;

    let _ = running.await;
    Ok(())
}

async fn name_lost_task(
    mut name_lost_stream: fdo::NameLostStream,
    abort_handle: futures::future::AbortHandle,
) {
    while let Some(name_lost) = name_lost_stream.next().await {
        let Ok(args) = name_lost.args() else {
            return;
        };
        if args.name == DBUS_NAME {
            eprintln!("'{}' name on bus lost. Exiting.", DBUS_NAME);
            abort_handle.abort();
            return;
        }
    }
}

async fn refresh_task(
    interface: zbus::object_server::InterfaceRef<CosmicAppletStatusNotifierWatcher>,
    abort_handle: futures::future::AbortHandle,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Initial tick, waiting for first client to connect
    interval.tick().await;
    loop {
        interval.tick().await;
        let mut watcher = interface.get_mut().await;
        if !watcher.has_client() {
            // No clients since last refresh; exit
            abort_handle.abort();
            return;
        }
        watcher.refresh();
    }
}
