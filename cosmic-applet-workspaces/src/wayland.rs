use calloop::channel::*;
use cosmic_panel_config::CosmicPanelOuput;
use cosmic_protocols::workspace::v1::client::{
    zcosmic_workspace_group_handle_v1::{self, ZcosmicWorkspaceGroupHandleV1},
    zcosmic_workspace_handle_v1::{self, ZcosmicWorkspaceHandleV1},
    zcosmic_workspace_manager_v1::{self, ZcosmicWorkspaceManagerV1},
};
use futures::{channel::mpsc, executor::block_on, SinkExt};
use sctk::event_loop::WaylandSource;
use std::{env, os::unix::net::UnixStream, path::PathBuf, str::FromStr, time::Duration};
use wayland_backend::client::ObjectId;
use wayland_client::{
    event_created_child,
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry::{self, WlRegistry},
    },
    ConnectError, Proxy,
};
use wayland_client::{Connection, Dispatch, QueueHandle};

#[derive(Debug, Clone)]
pub enum WorkspaceEvent {
    Activate(ObjectId),
    Scroll(f64),
}
pub type WorkspaceList = Vec<(String, Option<zcosmic_workspace_handle_v1::State>, ObjectId)>;

pub fn spawn_workspaces(tx: mpsc::Sender<WorkspaceList>) -> SyncSender<WorkspaceEvent> {
    let (workspaces_tx, workspaces_rx) = calloop::channel::sync_channel(100);

    if let Ok(Ok(conn)) = std::env::var("WAYLAND_DISPLAY")
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
            let output = std::env::var("COSMIC_PANEL_OUTPUT")
                .ok()
                .map(|output_str| match CosmicPanelOuput::from_str(&output_str) {
                    Ok(CosmicPanelOuput::Name(name)) => name,
                    _ => "".to_string(),
                })
                .unwrap_or_default();
            let mut event_loop = calloop::EventLoop::<State>::try_new().unwrap();
            let loop_handle = event_loop.handle();
            let event_queue = conn.new_event_queue::<State>();
            let qhandle = event_queue.handle();

            WaylandSource::new(event_queue)
                .expect("Failed to create wayland source")
                .insert(loop_handle)
                .unwrap();

            let display = conn.display();
            display.get_registry(&qhandle, ());

            let mut state = State {
                outputs_to_handle: Default::default(),
                wm_name: None,
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
                        if let Some(w) = state.workspace_groups.iter().find_map(|g| {
                            g.workspaces.iter().find(|w| w.workspace_handle.id() == id)
                        }) {
                            w.workspace_handle.activate();
                            state.workspace_manager.as_ref().unwrap().commit();
                        }
                    }
                    Event::Msg(WorkspaceEvent::Scroll(v)) => {
                        if let Some((w_g, w_i)) = state.workspace_groups.iter().find_map(|g| {
                            if g.output != state.expected_output {
                                return None;
                            }
                            g.workspaces
                                .iter()
                                .position(|w| {
                                    w.states
                                        .contains(&zcosmic_workspace_handle_v1::State::Active)
                                })
                                .map(|w_i| (g, w_i))
                        }) {
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
        eprintln!("ENV variable WAYLAND_DISPLAY is missing. Exiting...");
        std::process::exit(1);
    }

    workspaces_tx
}

#[derive(Debug, Clone)]
pub struct State {
    outputs_to_handle: Option<Vec<WlOutput>>,
    wm_name: Option<(u32, WlRegistry)>,
    running: bool,
    tx: mpsc::Sender<WorkspaceList>,
    configured_output: String,
    expected_output: Option<WlOutput>,
    workspace_manager: Option<ZcosmicWorkspaceManagerV1>,
    workspace_groups: Vec<WorkspaceGroup>,
}

impl State {
    // XXX
    pub fn workspace_list(
        &self,
    ) -> Vec<(String, Option<zcosmic_workspace_handle_v1::State>, ObjectId)> {
        self.workspace_groups
            .iter()
            .filter_map(|g| {
                // TODO remove none check when workspace groups receive output event
                if g.output.is_none() || g.output == self.expected_output {
                    Some(g.workspaces.iter().map(|w| {
                        (
                            w.name.clone(),
                            match &w.states {
                                x if x.contains(&zcosmic_workspace_handle_v1::State::Active) => {
                                    Some(zcosmic_workspace_handle_v1::State::Active)
                                }
                                x if x.contains(&zcosmic_workspace_handle_v1::State::Urgent) => {
                                    Some(zcosmic_workspace_handle_v1::State::Urgent)
                                }
                                x if x.contains(&zcosmic_workspace_handle_v1::State::Hidden) => {
                                    Some(zcosmic_workspace_handle_v1::State::Hidden)
                                }
                                _ => None,
                            },
                            w.workspace_handle.id(),
                        )
                    }))
                } else {
                    None
                }
            })
            .flatten()
            .collect()
    }
}

#[derive(Debug, Clone)]
struct WorkspaceGroup {
    workspace_group_handle: ZcosmicWorkspaceGroupHandleV1,
    output: Option<WlOutput>,
    workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    workspace_handle: ZcosmicWorkspaceHandleV1,
    name: String,
    coordinates: Vec<u32>,
    states: Vec<zcosmic_workspace_handle_v1::State>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version: _version,
        } = event
        {
            match &interface[..] {
                "zcosmic_workspace_manager_v1" => {
                    if let Some(outputs_to_handle) = state.outputs_to_handle.as_ref() {
                        if outputs_to_handle.is_empty() {
                            let workspace_manager =
                                registry.bind::<ZcosmicWorkspaceManagerV1, _, _>(name, 1, qh, ());
                            state.workspace_manager = Some(workspace_manager);
                            return;
                        }
                    }
                    // will be handled when outputs are done...
                    state.wm_name.replace((name, registry.clone()));
                }
                "wl_output" => {
                    let _output = registry.bind::<WlOutput, _, _>(name, 4, qh, ());
                    match state.outputs_to_handle.as_mut() {
                        Some(outputs_to_handle) => outputs_to_handle.push(_output),
                        None => {
                            state.outputs_to_handle.replace(vec![_output]);
                        }
                    };
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ZcosmicWorkspaceManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZcosmicWorkspaceManagerV1,
        event: zcosmic_workspace_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_workspace_manager_v1::Event::WorkspaceGroup { workspace_group } => {
                state.workspace_groups.push(WorkspaceGroup {
                    workspace_group_handle: workspace_group,
                    output: None,
                    workspaces: Vec::new(),
                });
            }
            zcosmic_workspace_manager_v1::Event::Done => {
                for group in &mut state.workspace_groups {
                    group.workspaces.sort_by(|w1, w2| {
                        w1.coordinates
                            .iter()
                            .zip(w2.coordinates.iter())
                            .rev()
                            .skip_while(|(coord1, coord2)| coord1 == coord2)
                            .next()
                            .map(|(coord1, coord2)| coord1.cmp(coord2))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                let _ = block_on(state.tx.send(state.workspace_list()));
            }
            zcosmic_workspace_manager_v1::Event::Finished => {
                state.workspace_manager.take();
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
        state: &mut Self,
        group: &ZcosmicWorkspaceGroupHandleV1,
        event: zcosmic_workspace_group_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_workspace_group_handle_v1::Event::OutputEnter { output } => {
                if let Some(group) = state
                    .workspace_groups
                    .iter_mut()
                    .find(|g| &g.workspace_group_handle == group)
                {
                    group.output = Some(output);
                }
            }
            zcosmic_workspace_group_handle_v1::Event::OutputLeave { output } => {
                if let Some(group) = state.workspace_groups.iter_mut().find(|g| {
                    &g.workspace_group_handle == group && g.output.as_ref() == Some(&output)
                }) {
                    group.output = None;
                }
            }
            zcosmic_workspace_group_handle_v1::Event::Workspace { workspace } => {
                if let Some(group) = state
                    .workspace_groups
                    .iter_mut()
                    .find(|g| &g.workspace_group_handle == group)
                {
                    group.workspaces.push(Workspace {
                        workspace_handle: workspace,
                        name: String::new(),
                        coordinates: Vec::new(),
                        states: Vec::new(),
                    })
                }
            }
            zcosmic_workspace_group_handle_v1::Event::Remove => {
                if let Some(group) = state
                    .workspace_groups
                    .iter()
                    .position(|g| &g.workspace_group_handle == group)
                {
                    state.workspace_groups.remove(group);
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
        state: &mut Self,
        workspace: &ZcosmicWorkspaceHandleV1,
        event: zcosmic_workspace_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_workspace_handle_v1::Event::Name { name } => {
                if let Some(w) = state.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    w.name = name;
                }
            }
            zcosmic_workspace_handle_v1::Event::Coordinates { coordinates } => {
                if let Some(w) = state.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    // wayland is host byte order
                    w.coordinates = coordinates
                        .chunks(4)
                        .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
                        .collect();
                }
            }
            zcosmic_workspace_handle_v1::Event::State {
                state: workspace_state,
            } => {
                if let Some(w) = state.workspace_groups.iter_mut().find_map(|g| {
                    g.workspaces
                        .iter_mut()
                        .find(|w| &w.workspace_handle == workspace)
                }) {
                    // wayland is host byte order
                    w.states = workspace_state
                        .chunks(4)
                        .map(|chunk| {
                            zcosmic_workspace_handle_v1::State::try_from(u32::from_ne_bytes(
                                chunk.try_into().unwrap(),
                            ))
                            .unwrap()
                        })
                        .collect();
                }
            }
            zcosmic_workspace_handle_v1::Event::Remove => {
                if let Some((g, w_i)) = state.workspace_groups.iter_mut().find_map(|g| {
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
        state: &mut Self,
        o: &WlOutput,
        e: wl_output::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match e {
            wl_output::Event::Name { name } if name == state.configured_output => {
                state.expected_output.replace(o.clone());
                // Necessary bc often the output is handled after the workspaces
                let _ = block_on(state.tx.send(state.workspace_list()));
            }
            wl_output::Event::Done => {
                let outputs_to_handle = state.outputs_to_handle.as_mut().unwrap();
                outputs_to_handle.retain(|o_to_handle| o != o_to_handle);
                if outputs_to_handle.is_empty() {
                    if let Some((wm_name, registry)) = state.wm_name.as_ref() {
                        let workspace_manager =
                            registry.bind::<ZcosmicWorkspaceManagerV1, _, _>(*wm_name, 1, qh, ());
                        state.workspace_manager = Some(workspace_manager);
                    }
                }
            }
            _ => {} // ignored
        }
    }
}
