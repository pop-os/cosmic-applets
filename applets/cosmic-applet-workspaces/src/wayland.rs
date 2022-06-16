use crate::utils::{Activate, Workspace};
use std::{
    os::unix::{net::UnixStream}, env, path::PathBuf,
};
use wayland_client::{ConnectError, DelegateDispatch, protocol::wl_registry};
use tokio::sync::mpsc;

use wayland_client::{
    Connection, Dispatch, QueueHandle,
};

/// Generated protocol definitions
mod generated {
    #![allow(dead_code,non_camel_case_types,unused_unsafe,unused_variables)]
    #![allow(non_upper_case_globals,non_snake_case,unused_imports)]
    #![allow(missing_docs, clippy::all)]

    pub mod client {
        //! Client-side API of this protocol
        use wayland_client;
        use wayland_client::protocol::*;

        pub mod __interfaces {
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("src/ext-workspace-unstable-v1.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_client_code!("src/ext-workspace-unstable-v1.xml");
    }
}

use generated::client::zext_workspace_manager_v1;

pub fn spawn_workspaces(tx: mpsc::Sender<Vec<Workspace>>) -> mpsc::Sender<Activate> {
    let (workspaces_tx, mut workspaces_rx) = mpsc::channel(100);
    if let Ok(Ok(conn)) = std::env::var("HOST_WAYLAND_DISPLAY")
        .map_err(anyhow::Error::msg)
        .map(|fd| {
            let mut socket_path = env::var_os("XDG_RUNTIME_DIR")
                .map(Into::<PathBuf>::into)
                .ok_or(ConnectError::NoCompositor)?;
            socket_path.push(env::var_os("WAYLAND_DISPLAY").ok_or(ConnectError::NoCompositor)?);

            Ok(UnixStream::connect(socket_path).map_err(|_| ConnectError::NoCompositor)?)
        })
        .and_then(|s| s.map(|s| Connection::from_socket(s).map_err(anyhow::Error::msg)))
    {
        std::thread::spawn(move || {
            let mut event_queue= conn.new_event_queue::<State>();
            let qhandle = event_queue.handle();

            let display = conn.display();
            display.get_registry(&qhandle, ()).unwrap();
        
            let mut state = State {
                workspace_manager: None,
                tx,
                running: true,
            };
                
            while state.running {
                event_queue.blocking_dispatch(&mut state).unwrap();
            }
        });
    } else {
        eprintln!("ENV variable HOST_WAYLAND_DISPLAY is missing. Exiting...");
        std::process::exit(1);
    }

    workspaces_tx
}



struct State {
    running: bool,
    tx: mpsc::Sender<Vec<Workspace>>,
    workspace_manager: Option<zext_workspace_manager_v1::ZextWorkspaceManagerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        &mut self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            println!("[{}] {} (v{})", name, interface, version);
            match &interface[..] {
                "zext_workspace_manager_v1" => {
                    println!("binding to workspace manager");
                    registry.bind::<zext_workspace_manager_v1::ZextWorkspaceManagerV1, _, _>(name, 1, qh, ()).unwrap();
                },
                _ => {}
            }
        }
    }
}

impl Dispatch<zext_workspace_manager_v1::ZextWorkspaceManagerV1, ()> for State {
    fn event(
        &mut self,
        _: &zext_workspace_manager_v1::ZextWorkspaceManagerV1,
        _: zext_workspace_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        todo!()
        // wl_compositor has no event
    }
}
