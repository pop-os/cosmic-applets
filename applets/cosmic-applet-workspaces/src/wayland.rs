use crate::utils::{Activate, Workspace};
use std::{
    num::ParseIntError,
    os::unix::prelude::RawFd,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
use wayland_client::{protocol::wl_registry, Display, GlobalManager};

mod generated {
    // The generated code tends to trigger a lot of warnings
    // so we isolate it into a very permissive module
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]

    pub mod client {
        // These imports are used by the generated code
        pub(crate) use wayland_commons::map::{Object, ObjectMetadata};
        pub(crate) use wayland_commons::smallvec;
        pub(crate) use wayland_commons::wire::{Argument, ArgumentType, Message, MessageDesc};
        pub(crate) use wayland_commons::{Interface, MessageGroup};
        pub(crate) use wayland_client::protocol::wl_output;
        pub(crate) use wayland_client::sys;
        pub(crate) use wayland_client::{AnonymousObject, Main, Proxy, ProxyMap};
        include!(concat!(env!("OUT_DIR"), "/ext_workspace.rs"));
    }
}

pub fn spawn_workspaces(tx: mpsc::Sender<Vec<Workspace>>) -> mpsc::Sender<Activate> {
    let (workspaces_tx, mut workspaces_rx) = mpsc::channel(100);
    if let Ok(display) = std::env::var("HOST_WAYLAND_DISPLAY")
        .map_err(anyhow::Error::msg)
        .and_then(|fd| Display::connect_to_name(fd).map_err(anyhow::Error::msg))
    {
        std::thread::spawn(move || {
            let mut event_queue = display.create_event_queue();
            let attached_display = display.attach(event_queue.token());
            let globals = GlobalManager::new(&attached_display);
            dbg!(event_queue.sync_roundtrip(&mut (), |_, _, _| unreachable!()));

            println!("Globals: ");
            for (name, interface, version) in globals.list() {
                println!("{}: {} (version {})", name, interface, version);
            }
        });
    } else {
        eprintln!("ENV variable HOST_WAYLAND_SOCKET is missing. Exiting...");
        std::process::exit(1);
    }

    workspaces_tx
}
