#![allow(non_snake_case)]

use std::sync::{Arc, Mutex};
use zbus::{dbus_interface, MessageHeader, Result, SignalContext};

use crate::dbus_service;

#[derive(Default)]
struct StatusNotifierWatcher {
    items: Arc<Mutex<Vec<String>>>,
}

#[dbus_interface(name = "org.kde.StatusNotifierWatcher")]
impl StatusNotifierWatcher {
    async fn RegisterStatusNotifierItem(
        &self,
        service: &str,
        #[zbus(header)] hdr: MessageHeader<'_>,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) {
        let service = format!("{}{}", hdr.sender().unwrap().unwrap(), service);
        Self::StatusNotifierItemRegistered(&ctxt, &service)
            .await
            .unwrap();

        // XXX emit unreigstered
        self.items.lock().unwrap().push(service);
    }

    fn RegisterStatusNotifierHost(&self, _service: &str) {
        // XXX emit registed/unregistered
    }

    #[dbus_interface(property)]
    fn RegisteredStatusNotifierItems(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
    }

    #[dbus_interface(property)]
    fn IsStatusNotifierHostRegistered(&self) -> bool {
        true
    }

    #[dbus_interface(property)]
    fn ProtocolVersion(&self) -> i32 {
        0
    }

    #[dbus_interface(signal)]
    async fn StatusNotifierItemRegistered(ctxt: &SignalContext<'_>, service: &str) -> Result<()>;

    #[dbus_interface(signal)]
    async fn StatusNotifierItemUnregistered(ctxt: &SignalContext<'_>, service: &str) -> Result<()>;

    #[dbus_interface(signal)]
    async fn StatusNotifierHostRegistered(ctxt: &SignalContext<'_>) -> Result<()>;

    #[dbus_interface(signal)]
    async fn StatusNotifierHostUnregistered(ctxt: &SignalContext<'_>) -> Result<()>;
}

pub async fn start() {
    if let Err(err) = dbus_service::create("org.kde.StatusNotifierWatcher", |builder| {
        builder.serve_at("/StatusNotifierWatcher", StatusNotifierWatcher::default())
    })
    .await
    {
        eprintln!("Failed to start `StatusNotifierWatcher` service: {}", err);
    }
}
