use crate::{
    utils::{Activate, WorkspaceEvent},
    wayland_source::WaylandSource,
};
use cosmic_panel_config::CosmicPanelConfig;
use gtk4::glib;
use std::{
    collections::HashMap, env, hash::Hash, mem, os::unix::net::UnixStream, path::PathBuf,
    sync::Arc, time::Duration,
};
use tokio::sync::mpsc;
use wayland_backend::client::ObjectData;
use wayland_client::{
    event_created_child,
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry,
    },
    ConnectError, Proxy,
};
use cosmic_protocols::workspace::v1::client::{
    zcosmic_workspace_manager_v1::{self, ZcosmicWorkspaceManagerV1},
    zcosmic_workspace_group_handle_v1::{self, ZcosmicWorkspaceGroupHandleV1},
    zcosmic_workspace_handle_v1::{self, ZcosmicWorkspaceHandleV1},
};
use wayland_client::{Connection, Dispatch, QueueHandle};
use calloop::channel::*;

pub fn spawn_workspaces(tx: glib::Sender<State>) -> SyncSender<WorkspaceEvent> {
    let (workspaces_tx, mut workspaces_rx) = calloop::channel::sync_channel(100);

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
            let output = CosmicPanelConfig::load_from_env()
                .unwrap_or_default()
                .output;
            let mut event_loop = calloop::EventLoop::<State>::try_new().unwrap();
            let loop_handle = event_loop.handle();
            let event_queue = conn.new_event_queue::<State>();
            let qhandle = event_queue.handle();

            WaylandSource::new(event_queue)
                .expect("Failed to create wayland source")
                .insert(loop_handle)
                .unwrap();

            let display = conn.display();
            display.get_registry(&qhandle, ()).unwrap();

            let mut state = State {
                workspace_manager: None,
                workspace_groups: Vec::new(),
                configured_output: output,
                expected_output: None,
                tx,
                running: true,
            };
            let loop_handle = event_loop.handle();
            loop_handle
                .insert_source(workspaces_rx, |e, _, state| match e {
                    Event::Msg(WorkspaceEvent::Activate(id)) => {
                        if let Some(w) = state
                            .workspace_groups
                            .iter()
                            .find_map(|g| g.workspaces.iter().find(|w| w.name == id))
                        {
                            w.workspace_handle.activate();
                            state.workspace_manager.as_ref().unwrap().commit();
                        }
                    }
                    Event::Msg(WorkspaceEvent::Scroll(v)) => {
                        if let Some((w_g, w_i)) = state
                            .workspace_groups
                            .iter()
                            .enumerate()
                            .find_map(|(g_i, g)| {
                                g.workspaces
                                    .iter()
                                    .position(|w| w.state == 0)
                                    .map(|w_i| (g, w_i))
                            })
                        {
                            let max_w = w_g.workspaces.len().wrapping_sub(1);
                            let d_i = if v > 0.0 {
                                if w_i == max_w {
                                    0
                                } else {
                                    w_i.wrapping_add(1)
                                }
                            } else {
                                if w_i == 0 {
                                    max_w
                                } else {
                                    w_i.wrapping_sub(1)
                                }
                            };
                            if let Some(w) = w_g.workspaces.get(d_i) {
                                w.workspace_handle.activate();
                                state.workspace_manager.as_ref().unwrap().commit();
                            }
                        }
                    }
                    Event::Closed => {
                        if let Some(workspace_manager) = &mut state.workspace_manager {
                            for g in &mut state.workspace_groups {
                                g.workspace_group_handle.destroy();
                            }
                            workspace_manager.stop();
                        }
                    }
                })
                .unwrap();
            while state.running {
                event_loop
                    .dispatch(Duration::from_millis(16), &mut state)
                    .unwrap();
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
    tx: glib::Sender<State>,
    configured_output: String,
    expected_output: Option<WlOutput>,
    workspace_manager: Option<ZcosmicWorkspaceManagerV1>,
    workspace_groups: Vec<WorkspaceGroup>,
}

impl State {
    // XXX
    pub fn workspace_list(&self) -> impl Iterator<Item = (String, u32)> + '_ {
        self.workspace_groups
            .iter()
            .filter_map(|g| {
                if g.output == self.expected_output {
                    Some(g.workspaces.iter().map(|w| (w.name.clone(), w.state)))
                } else {
                    None
                }
            })
            .flatten()
    }
}

#[derive(Debug, Clone)]
struct WorkspaceGroup {
    workspace_group_handle: ZcosmicWorkspaceGroupHandleV1,
    output: Option<WlOutput>,
    workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone)]
struct Workspace {
    workspace_handle: ZcosmicWorkspaceHandleV1,
    name: String,
    coordinates: Vec<u8>,
    state: u32,
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
            match &interface[..] {
                "zcosmic_workspace_manager_v1" => {
                    let workspace_manager = registry
                        .bind::<ZcosmicWorkspaceManagerV1, _, _>(
                            name,
                            1,
                            qh,
                            (),
                        )
                        .unwrap();
                    self.workspace_manager = Some(workspace_manager);
                }
                "wl_output" => {
                    registry.bind::<WlOutput, _, _>(name, 1, qh, ()).unwrap();
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ZcosmicWorkspaceManagerV1, ()> for State {
    fn event(
        &mut self,
        _: &ZcosmicWorkspaceManagerV1,
        event: zcosmic_workspace_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_workspace_manager_v1::Event::WorkspaceGroup { workspace_group } => {
                self.workspace_groups.push(WorkspaceGroup {
                    workspace_group_handle: workspace_group,
                    output: None,
                    workspaces: Vec::new(),
                });
            }
            zcosmic_workspace_manager_v1::Event::Done => {
                let _ = self.tx.send(self.clone());
            }
            zcosmic_workspace_manager_v1::Event::Finished => {
                self.workspace_manager.take();
            }
            _ => {}
        }
    }

    event_created_child!(State, ZcosmicWorkspaceManagerV1, [
        0 => (ZcosmicWorkspaceGroupHandleV1, ())
    ]);
}

impl Dispatch<ZcosmicWorkspaceGroupHandleV1, ()> for State {
    fn event(
        &mut self,
        group: &ZcosmicWorkspaceGroupHandleV1,
        event: zcosmic_workspace_group_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_workspace_group_handle_v1::Event::OutputEnter { output } => {
                if let Some(group) = self
                    .workspace_groups
                    .iter_mut()
                    .find(|g| &g.workspace_group_handle == group)
                {
                    group.output = Some(output);
                }
            }
            zcosmic_workspace_group_handle_v1::Event::OutputLeave { output } => {
                if let Some(group) = self.workspace_groups.iter_mut().find(|g| {
                    &g.workspace_group_handle == group && g.output.as_ref() == Some(&output)
                }) {
                    group.output = None;
                }
            }
            zcosmic_workspace_group_handle_v1::Event::Workspace { workspace } => {
                if let Some(group) = self
                    .workspace_groups
                    .iter_mut()
                    .find(|g| &g.workspace_group_handle == group)
                {
                    group.workspaces.push(Workspace {
                        workspace_handle: workspace,
                        name: String::new(),
                        coordinates: Vec::new(),
                        state: 4,
                    })
                }
            }
            zcosmic_workspace_group_handle_v1::Event::Remove => {
                if let Some(group) = self
                    .workspace_groups
                    .iter()
                    .position(|g| &g.workspace_group_handle == group)
                {
                    self.workspace_groups.remove(group);
                }
            }
            _ => {}
        }
    }

    event_created_child!(State, ZcosmicWorkspaceGroupHandleV1, [
        3 => (ZcosmicWorkspaceHandleV1, ())
    ]);
}

impl Dispatch<ZcosmicWorkspaceHandleV1, ()> for State {
    fn event(
        &mut self,
        workspace: &ZcosmicWorkspaceHandleV1,
        event: zcosmic_workspace_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_workspace_handle_v1::Event::Name { name } => {
                if let Some(w) = self.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    w.name = name;
                }
            }
            zcosmic_workspace_handle_v1::Event::Coordinates { coordinates } => {
                if let Some(w) = self.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    w.coordinates = coordinates;
                }
            }
            zcosmic_workspace_handle_v1::Event::State { state } => {
                if let Some(w) = self.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    if state.len() == 4 {
                        // XXX is it little endian??
                        w.state = u32::from_le_bytes(state.try_into().unwrap());
                    } else {
                        w.state = 3;
                    }
                }
            }
            zcosmic_workspace_handle_v1::Event::Remove => {
                if let Some((g, w_i)) = self.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .position(|w| &w.workspace_handle == workspace)
                        .map(|p| (g, p))
                }) {
                    g.workspaces.remove(w_i);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, ()> for State {
    fn event(
        &mut self,
        o: &WlOutput,
        e: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            wl_output::Event::Name { name } if name == self.configured_output => {
                self.expected_output.replace(o.clone());
            }
            _ => {} // ignored
        }
    }
}
