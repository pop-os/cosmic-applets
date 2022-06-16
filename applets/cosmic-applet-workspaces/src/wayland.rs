use crate::utils::{Activate};
use std::{env, os::unix::net::UnixStream, path::PathBuf};
use tokio::sync::mpsc;
use wayland_client::{
    protocol::{wl_output::{WlOutput, self}, wl_registry},
    ConnectError,
};

use wayland_client::{Connection, Dispatch, QueueHandle};

/// Generated protocol definitions
mod generated {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
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

use self::generated::client::{
    zext_workspace_group_handle_v1::{self, ZextWorkspaceGroupHandleV1},
    zext_workspace_handle_v1::{self, ZextWorkspaceHandleV1},
};

pub fn spawn_workspaces(tx: mpsc::Sender<State>) -> mpsc::Sender<Activate> {
    let (workspaces_tx, mut workspaces_rx) = mpsc::channel(100);
    if let Ok(Ok(conn)) = std::env::var("HOST_WAYLAND_DISPLAY")
        .map_err(anyhow::Error::msg)
        .map(|display_str| {
            let mut socket_path = env::var_os("XDG_RUNTIME_DIR")
                .map(Into::<PathBuf>::into)
                .ok_or(ConnectError::NoCompositor)?;
            socket_path.push(display_str);

            Ok(UnixStream::connect(socket_path).map_err(|_| ConnectError::NoCompositor)?)
        })
        .and_then(|s| s.map(|s| Connection::from_socket(s).map_err(anyhow::Error::msg)))
    {
        std::thread::spawn(move || {
            let mut event_queue = conn.new_event_queue::<State>();
            let qhandle = event_queue.handle();

            let display = conn.display();
            display.get_registry(&qhandle, ()).unwrap();

            let mut state = State {
                workspace_manager: None,
                workspace_groups: Vec::new(),
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

#[derive(Debug, Clone)]
pub struct State {
    running: bool,
    tx: mpsc::Sender<State>,
    workspace_manager: Option<zext_workspace_manager_v1::ZextWorkspaceManagerV1>,
    workspace_groups: Vec<WorkspaceGroup>,
}

#[derive(Debug, Clone)]
struct WorkspaceGroup {
    workspace_group_handle: ZextWorkspaceGroupHandleV1,
    output: Option<WlOutput>,
    workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone)]
struct Workspace {
    workspace_handle: ZextWorkspaceHandleV1,
    name: String,
    coordinates: Vec<u8>,
    state: Vec<u8>,
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
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            println!("[{}] {} (v{})", name, interface, version);
            match &interface[..] {
                "zext_workspace_manager_v1" => {
                    println!("binding to workspace manager");
                    let workspace_manager = registry
                        .bind::<zext_workspace_manager_v1::ZextWorkspaceManagerV1, _, _>(
                            name,
                            1,
                            qh,
                            (),
                        )
                        .unwrap();
                    self.workspace_manager = Some(workspace_manager);
                }
                "wl_output" => {
                    println!("binding to output");
                    registry.bind::<WlOutput, _, _>(name, 1, qh, ()).unwrap();
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zext_workspace_manager_v1::ZextWorkspaceManagerV1, ()> for State {
    fn event(
        &mut self,
        _: &zext_workspace_manager_v1::ZextWorkspaceManagerV1,
        event: zext_workspace_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zext_workspace_manager_v1::Event::WorkspaceGroup { workspace_group } => {
                self.workspace_groups.push(WorkspaceGroup {
                    workspace_group_handle: workspace_group,
                    output: None,
                    workspaces: Vec::new(),
                });
            }
            zext_workspace_manager_v1::Event::Done => {
                // TODO
                println!("sending event with workspace list state");
                let _ = self.tx.send(self.clone());
            }
            zext_workspace_manager_v1::Event::Finished => {
                self.workspace_manager.take();
            }
        }
        // wl_compositor has no event
    }
}

impl Dispatch<ZextWorkspaceGroupHandleV1, ()> for State {
    fn event(
        &mut self,
        group: &ZextWorkspaceGroupHandleV1,
        event: zext_workspace_group_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zext_workspace_group_handle_v1::Event::OutputEnter { output } => {
                if let Some(group) = self
                    .workspace_groups
                    .iter_mut()
                    .find(|g| &g.workspace_group_handle == group)
                {
                    group.output = Some(output);
                }
            }
            zext_workspace_group_handle_v1::Event::OutputLeave { output } => {
                if let Some(group) = self.workspace_groups.iter_mut().find(|g| {
                    &g.workspace_group_handle == group && g.output.as_ref() == Some(&output)
                }) {
                    group.output = None;
                }
            }
            zext_workspace_group_handle_v1::Event::Workspace { workspace } => {
                if let Some(group) = self
                    .workspace_groups
                    .iter_mut()
                    .find(|g| &g.workspace_group_handle == group)
                {
                    group.workspaces.push(Workspace {
                        workspace_handle: workspace,
                        name: String::new(),
                        coordinates: Vec::new(),
                        state: Vec::new(),
                    })
                }
            }
            zext_workspace_group_handle_v1::Event::Remove => {
                if let Some(group) = self
                    .workspace_groups
                    .iter()
                    .position(|g| &g.workspace_group_handle == group)
                {
                    self.workspace_groups.remove(group);
                }
            }
        }
    }
}

impl Dispatch<ZextWorkspaceHandleV1, ()> for State {
    fn event(
        &mut self,
        workspace: &ZextWorkspaceHandleV1,
        event: zext_workspace_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zext_workspace_handle_v1::Event::Name { name } => {
                if let Some(w) = self.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    w.name = name;
                }
            }
            zext_workspace_handle_v1::Event::Coordinates { coordinates } => {
                if let Some(w) = self.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    w.coordinates = coordinates;
                }
            }
            zext_workspace_handle_v1::Event::State { state } => {
                if let Some(w) = self.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    w.state = state;
                }
            }
            zext_workspace_handle_v1::Event::Remove => {
                if let Some((g, w_i)) = self.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .position(|w| &w.workspace_handle == workspace)
                        .map(|p| (g, p))
                }) {
                    g.workspaces.remove(w_i);
                }
            }
        }
    }
}

impl Dispatch<WlOutput, ()> for State {
    fn event(
        &mut self,
        _: &WlOutput,
        _: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}
