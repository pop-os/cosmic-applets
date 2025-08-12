// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use calloop::channel::*;
use cctk::{
    sctk::{
        self,
        output::{OutputHandler, OutputState},
        reexports::{
            calloop, calloop_wayland_source::WaylandSource, client as wayland_client,
            protocols::ext::workspace::v1::client::ext_workspace_handle_v1,
        },
        registry::{ProvidesRegistryState, RegistryState},
    },
    toplevel_info::{ToplevelInfoHandler, ToplevelInfoState},
    wayland_client::WEnum,
    wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    workspace::{WorkspaceHandler, WorkspaceState},
};
use cosmic::iced::futures;
use cosmic_protocols::workspace::v2::client::zcosmic_workspace_handle_v2::TilingState;
use futures::{SinkExt, channel::mpsc, executor::block_on};
use std::{
    collections::HashSet,
    os::{
        fd::{FromRawFd, RawFd},
        unix::net::UnixStream,
    },
};
use tracing::error;
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::wl_output::{self, WlOutput},
};

#[derive(Debug, Clone)]
pub enum AppRequest {
    TilingState(TilingState),
    DefaultBehavior(TilingState),
}

pub fn spawn_workspaces(tx: mpsc::Sender<TilingState>) -> SyncSender<AppRequest> {
    let (workspaces_tx, workspaces_rx) = calloop::channel::sync_channel(100);

    let socket = std::env::var("X_PRIVILEGED_WAYLAND_SOCKET")
        .ok()
        .and_then(|fd| {
            fd.parse::<RawFd>()
                .ok()
                .map(|fd| unsafe { UnixStream::from_raw_fd(fd) })
        });

    let conn = if let Some(socket) = socket {
        Connection::from_socket(socket)
    } else {
        Connection::connect_to_env()
    }
    .map_err(anyhow::Error::msg);

    if let Ok(conn) = conn {
        std::thread::spawn(move || {
            let configured_output = std::env::var("COSMIC_PANEL_OUTPUT")
                .ok()
                .unwrap_or_default();
            let mut event_loop = calloop::EventLoop::<State>::try_new().unwrap();
            let loop_handle = event_loop.handle();
            let (globals, event_queue) = registry_queue_init(&conn).unwrap();
            let qhandle = event_queue.handle();

            WaylandSource::new(conn, event_queue)
                .insert(loop_handle)
                .unwrap();

            let registry_state = RegistryState::new(&globals);
            let mut state = State {
                // Must be before `WorkspaceState`
                output_state: OutputState::new(&globals, &qhandle),
                configured_output,
                workspace_state: WorkspaceState::new(&registry_state, &qhandle),
                toplevel_info_state: ToplevelInfoState::new(&registry_state, &qhandle),
                workspaces_with_previous_toplevel: HashSet::new(),
                registry_state,
                expected_output: None,
                tx,
                running: true,
                have_workspaces: false,
            };
            let loop_handle = event_loop.handle();
            loop_handle
                .insert_source(workspaces_rx, |e, _, state| match e {
                    Event::Msg(AppRequest::TilingState(autotile)) => {
                        if let Some(w) = state.workspace_state.workspace_groups().find_map(|g| {
                            if let Some(o) = state.expected_output.as_ref() {
                                if !g.outputs.contains(o) {
                                    return None;
                                }
                            }
                            g.workspaces
                                .iter()
                                .filter_map(|handle| state.workspace_state.workspace_info(handle))
                                .find(|w| w.state.contains(ext_workspace_handle_v1::State::Active))
                        }) {
                            if let Some(cosmic_handle) = &w.cosmic_handle {
                                cosmic_handle.set_tiling_state(autotile);
                                state
                                    .workspace_state
                                    .workspace_manager()
                                    .get()
                                    .unwrap()
                                    .commit();
                            }
                        }
                    }
                    Event::Msg(AppRequest::DefaultBehavior(tiling)) => {
                        for w in state
                            .workspace_state
                            .workspace_groups()
                            .flat_map(|g| g.workspaces.iter())
                            .filter_map(|handle| state.workspace_state.workspace_info(handle))
                            .filter(|w| {
                                !state.workspaces_with_previous_toplevel.contains(&w.handle)
                            })
                        {
                            if let Some(cosmic_handle) = &w.cosmic_handle {
                                cosmic_handle.set_tiling_state(tiling);
                            }
                        }
                        state
                            .workspace_state
                            .workspace_manager()
                            .get()
                            .unwrap()
                            .commit();
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
                event_loop.dispatch(None, &mut state).unwrap();
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
    tx: mpsc::Sender<TilingState>,
    configured_output: String,
    expected_output: Option<WlOutput>,
    output_state: OutputState,
    registry_state: RegistryState,
    workspace_state: WorkspaceState,
    toplevel_info_state: ToplevelInfoState,
    workspaces_with_previous_toplevel: HashSet<ext_workspace_handle_v1::ExtWorkspaceHandleV1>,
    have_workspaces: bool,
}

impl State {
    pub fn tiling_state(&self) -> Option<TilingState> {
        self.workspace_state.workspace_groups().find_map(|g| {
            if g.outputs
                .iter()
                .any(|o| Some(o) == self.expected_output.as_ref())
            {
                g.workspaces
                    .iter()
                    .filter_map(|handle| self.workspace_state.workspace_info(handle))
                    .find_map(|w| {
                        if w.state.contains(ext_workspace_handle_v1::State::Active) {
                            w.tiling.and_then(|e| match e {
                                WEnum::Value(v) => Some(v),
                                _ => {
                                    error!("No tiling state for the workspace");
                                    None
                                }
                            })
                        } else {
                            None
                        }
                    })
            } else {
                None
            }
        })
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
                if let Some(s) = self.tiling_state() {
                    let _ = block_on(self.tx.send(s));
                }
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
        if let Some(s) = self.tiling_state() {
            let _ = block_on(self.tx.send(s));
        }
    }
}

impl ToplevelInfoHandler for State {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info_state
    }

    fn new_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        let Some(w) = self
            .toplevel_info_state
            .info(&toplevel)
            .map(|t| t.workspace.clone())
        else {
            return;
        };
        self.workspaces_with_previous_toplevel.extend(w);
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        let Some(w) = self
            .toplevel_info_state
            .info(&toplevel)
            .map(|t| t.workspace.clone())
        else {
            return;
        };
        self.workspaces_with_previous_toplevel.extend(w);
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _toplevel: &ExtForeignToplevelHandleV1,
    ) {
    }
}

cctk::delegate_toplevel_info!(State);
cctk::delegate_workspace!(State);
sctk::delegate_output!(State);
sctk::delegate_registry!(State);
