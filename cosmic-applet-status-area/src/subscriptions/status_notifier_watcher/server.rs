// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

// TODO: `g_bus_own_name` like abstraction in zbus

#![allow(non_snake_case)]

use futures::prelude::*;
use zbus::{
    Result,
    fdo::{DBusProxy, RequestNameFlags, RequestNameReply},
    message::Header,
    names::{BusName, UniqueName, WellKnownName},
    object_server::SignalEmitter,
};

const NAME: WellKnownName =
    WellKnownName::from_static_str_unchecked("org.kde.StatusNotifierWatcher");
const OBJECT_PATH: &str = "/StatusNotifierWatcher";

#[derive(Default)]
struct StatusNotifierWatcher {
    items: Vec<(UniqueName<'static>, String)>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl StatusNotifierWatcher {
    async fn register_status_notifier_item(
        &mut self,
        service: &str,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(signal_emitter)] ctxt: SignalEmitter<'_>,
    ) {
        let sender = hdr.sender().unwrap();
        let service = if service.starts_with('/') {
            format!("{sender}{service}")
        } else {
            service.to_string()
        };

        // Ignore duplicate
        if !self.items.iter().any(|(a, b)| (a, b) == (sender, &service)) {
            Self::status_notifier_item_registered(&ctxt, &service)
                .await
                .unwrap();

            self.items.push((sender.to_owned(), service));
        }
    }

    fn register_status_notifier_host(&self, _service: &str) {
        // XXX emit registed/unregistered
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.iter().map(|(_, x)| x.clone()).collect()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(ctxt: &SignalEmitter<'_>, service: &str)
    -> Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        ctxt: &SignalEmitter<'_>,
        service: &str,
    ) -> Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(ctxt: &SignalEmitter<'_>) -> Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_unregistered(ctxt: &SignalEmitter<'_>) -> Result<()>;
}

pub async fn create_service(connection: &zbus::Connection) -> zbus::Result<()> {
    connection
        .object_server()
        .at(OBJECT_PATH, StatusNotifierWatcher::default())
        .await?;
    let interface = connection
        .object_server()
        .interface::<_, StatusNotifierWatcher>(OBJECT_PATH)
        .await
        .unwrap();
    let dbus_proxy = DBusProxy::new(connection).await?;
    let mut name_owner_changed_stream = dbus_proxy.receive_name_owner_changed().await?;

    let flags = RequestNameFlags::AllowReplacement.into();
    if dbus_proxy.request_name(NAME.as_ref(), flags).await? == RequestNameReply::InQueue {
        eprintln!("Bus name '{NAME}' already owned");
    }

    let connection = connection.clone();
    tokio::spawn(async move {
        let mut have_bus_name = false;
        let unique_name = connection.unique_name().map(|x| x.as_ref());
        while let Some(evt) = name_owner_changed_stream.next().await {
            let Ok(args) = evt.args() else {
                continue;
            };
            if args.name.as_ref() == NAME {
                if args.new_owner.as_ref() == unique_name.as_ref() {
                    eprintln!("Acquired bus name: {NAME}");
                    have_bus_name = true;
                } else if have_bus_name {
                    eprintln!("Lost bus name: {NAME}");
                    have_bus_name = false;
                }
            } else if let BusName::Unique(name) = &args.name {
                if args.new_owner.is_none() {
                    let mut interface = interface.get_mut().await;
                    if let Some(idx) = interface
                        .items
                        .iter()
                        .position(|(unique_name, _)| unique_name == name)
                    {
                        let ctxt = SignalEmitter::new(&connection, OBJECT_PATH).unwrap();
                        let service = interface.items.remove(idx).1;
                        StatusNotifierWatcher::status_notifier_item_unregistered(&ctxt, &service)
                            .await
                            .unwrap();
                    }
                }
            }
        }
    });

    Ok(())
}
