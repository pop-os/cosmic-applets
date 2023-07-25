use calloop::channel::*;
use cctk::{
    sctk::{
        self,
        output::{OutputHandler, OutputState},
        reexports::{
            calloop,
            client::{self as wayland_client},
        },
        registry::{ProvidesRegistryState, RegistryState},
    },
    workspace::{WorkspaceHandler, WorkspaceState},
};
use cosmic_protocols::workspace::v1::client::zcosmic_workspace_handle_v1;
use futures::{channel::mpsc, executor::block_on, SinkExt};
use std::{env, os::unix::net::UnixStream, path::PathBuf, time::Duration};
use wayland_client::backend::ObjectId;
use wayland_client::{
    globals::registry_queue_init,
    protocol::wl_output::{self, WlOutput},
    ConnectError, Proxy, WaylandSource,
};
use wayland_client::{Connection, QueueHandle, WEnum};

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
            let configured_output = std::env::var("COSMIC_PANEL_OUTPUT")
                .ok()
                .unwrap_or_default();
            let mut event_loop = calloop::EventLoop::<State>::try_new().unwrap();
            let loop_handle = event_loop.handle();
            let (globals, event_queue) = registry_queue_init(&conn).unwrap();
            let qhandle = event_queue.handle();

            WaylandSource::new(event_queue)
                .expect("Failed to create wayland source")
                .insert(loop_handle)
                .unwrap();

            let registry_state = RegistryState::new(&globals);
            let mut state = State {
                // Must be before `WorkspaceState`
                output_state: OutputState::new(&globals, &qhandle),
                configured_output,
                workspace_state: WorkspaceState::new(&registry_state, &qhandle),
                registry_state,
                expected_output: None,
                tx,
                running: true,
                have_workspaces: false,
            };
            let loop_handle = event_loop.handle();
            loop_handle
                .insert_source(workspaces_rx, |e, _, state| match e {
                    Event::Msg(WorkspaceEvent::Activate(id)) => {
                        if let Some(w) = state
                            .workspace_state
                            .workspace_groups()
                            .iter()
                            .find_map(|g| g.workspaces.iter().find(|w| w.handle.id() == id))
                        {
                            w.handle.activate();
                            state
                                .workspace_state
                                .workspace_manager()
                                .get()
                                .unwrap()
                                .commit();
                        }
                    }
                    Event::Msg(WorkspaceEvent::Scroll(v)) => {
                        if let Some((w_g, w_i)) = state
                            .workspace_state
                            .workspace_groups()
                            .iter()
                            .find_map(|g| {
                                if !g
                                    .outputs
                                    .iter()
                                    .any(|o| Some(o) == state.expected_output.as_ref())
                                {
                                    return None;
                                }
                                g.workspaces
                                    .iter()
                                    .position(|w| {
                                        w.state.contains(&WEnum::Value(
                                            zcosmic_workspace_handle_v1::State::Active,
                                        ))
                                    })
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
                            } else if w_i == 0 {
                                max_w
                            } else {
                                w_i.wrapping_sub(1)
                            };
                            if let Some(w) = w_g.workspaces.get(d_i) {
                                w.handle.activate();
                                state
                                    .workspace_state
                                    .workspace_manager()
                                    .get()
                                    .unwrap()
                                    .commit();
                            }
                        }
                    }
                    Event::Closed => {
                        if let Ok(workspace_manager) =
                            state.workspace_state.workspace_manager().get()
                        {
                            for g in state.workspace_state.workspace_groups() {
                                g.handle.destroy();
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

#[derive(Debug)]
pub struct State {
    running: bool,
    tx: mpsc::Sender<WorkspaceList>,
    configured_output: String,
    expected_output: Option<WlOutput>,
    output_state: OutputState,
    registry_state: RegistryState,
    workspace_state: WorkspaceState,
    have_workspaces: bool,
}

impl State {
    pub fn workspace_list(
        &self,
    ) -> Vec<(String, Option<zcosmic_workspace_handle_v1::State>, ObjectId)> {
        self.workspace_state
            .workspace_groups()
            .iter()
            .filter_map(|g| {
                if g.outputs
                    .iter()
                    .any(|o| Some(o) == self.expected_output.as_ref())
                {
                    Some(g.workspaces.iter().map(|w| {
                        (
                            w.name.clone(),
                            match &w.state {
                                x if x.contains(&WEnum::Value(
                                    zcosmic_workspace_handle_v1::State::Active,
                                )) =>
                                {
                                    Some(zcosmic_workspace_handle_v1::State::Active)
                                }
                                x if x.contains(&WEnum::Value(
                                    zcosmic_workspace_handle_v1::State::Urgent,
                                )) =>
                                {
                                    Some(zcosmic_workspace_handle_v1::State::Urgent)
                                }
                                x if x.contains(&WEnum::Value(
                                    zcosmic_workspace_handle_v1::State::Hidden,
                                )) =>
                                {
                                    Some(zcosmic_workspace_handle_v1::State::Hidden)
                                }
                                _ => None,
                            },
                            w.handle.id(),
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

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    sctk::registry_handlers![OutputState,];
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let info = self.output_state.info(&output).unwrap();
        if info.name.as_deref() == Some(&self.configured_output) {
            self.expected_output = Some(output);
            if self.have_workspaces {
                let _ = block_on(self.tx.send(self.workspace_list()));
            }
        }
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl WorkspaceHandler for State {
    fn workspace_state(&mut self) -> &mut WorkspaceState {
        &mut self.workspace_state
    }

    fn done(&mut self) {
        self.have_workspaces = true;
        let _ = block_on(self.tx.send(self.workspace_list()));
    }
}

cctk::delegate_workspace!(State);
sctk::delegate_output!(State);
sctk::delegate_registry!(State);
