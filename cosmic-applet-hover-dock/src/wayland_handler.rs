// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::os::{
    fd::{FromRawFd, RawFd},
    unix::net::UnixStream,
};

use cctk::cosmic_protocols::toplevel_management::v1::client::zcosmic_toplevel_manager_v1;
use cctk::{
    self,
    cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    sctk::{
        self,
        reexports::{calloop, calloop_wayland_source::WaylandSource},
        seat::{SeatHandler, SeatState},
    },
    toplevel_info::{ToplevelInfoHandler, ToplevelInfoState},
    toplevel_management::{ToplevelManagerHandler, ToplevelManagerState},
    wayland_client::{
        Connection, QueueHandle, WEnum, globals::registry_queue_init, protocol::wl_seat::WlSeat,
    },
    wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
};
use futures::channel::mpsc::UnboundedSender;
use sctk::registry::{ProvidesRegistryState, RegistryState};

use crate::wayland_subscription::{ToplevelUpdate, WaylandRequest, WaylandUpdate};

struct AppData {
    exit: bool,
    tx: UnboundedSender<WaylandUpdate>,
    registry_state: RegistryState,
    seat_state: SeatState,
    toplevel_info_state: ToplevelInfoState,
    toplevel_manager_state: ToplevelManagerState,
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    sctk::registry_handlers!();
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: sctk::seat::Capability,
    ) {
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: sctk::seat::Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl ToplevelManagerHandler for AppData {
    fn toplevel_manager_state(&mut self) -> &mut ToplevelManagerState {
        &mut self.toplevel_manager_state
    }
    fn capabilities(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: Vec<WEnum<zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1>>,
    ) {
    }
}

impl AppData {
    fn cosmic_toplevel(
        &self,
        handle: &ExtForeignToplevelHandleV1,
    ) -> Option<ZcosmicToplevelHandleV1> {
        self.toplevel_info_state
            .info(handle)?
            .cosmic_toplevel
            .clone()
    }

    fn send_info(&self, handle: &ExtForeignToplevelHandleV1, is_new: bool) {
        if let Some(info) = self.toplevel_info_state.info(handle) {
            let update = if is_new {
                ToplevelUpdate::Add(info.clone())
            } else {
                ToplevelUpdate::Update(info.clone())
            };
            let _ = self
                .tx
                .unbounded_send(WaylandUpdate::Toplevel(Box::new(update)));
        }
    }
}

impl ToplevelInfoHandler for AppData {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info_state
    }
    fn new_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        handle: &ExtForeignToplevelHandleV1,
    ) {
        self.send_info(handle, true);
    }
    fn update_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        handle: &ExtForeignToplevelHandleV1,
    ) {
        self.send_info(handle, false);
    }
    fn toplevel_closed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        handle: &ExtForeignToplevelHandleV1,
    ) {
        let _ = self
            .tx
            .unbounded_send(WaylandUpdate::Toplevel(Box::new(ToplevelUpdate::Remove(
                handle.clone(),
            ))));
    }
}

pub fn wayland_handler(
    tx: UnboundedSender<WaylandUpdate>,
    rx: calloop::channel::Channel<WaylandRequest>,
) {
    let socket = std::env::var("X_PRIVILEGED_WAYLAND_SOCKET")
        .ok()
        .and_then(|fd| fd.parse::<RawFd>().ok())
        .map(|fd| unsafe { UnixStream::from_raw_fd(fd) });
    let conn = socket.map_or_else(
        || Connection::connect_to_env().expect("connect to Wayland compositor"),
        |socket| Connection::from_socket(socket).expect("connect to privileged Wayland socket"),
    );
    let (globals, event_queue) = registry_queue_init(&conn).expect("initialize Wayland registry");
    let mut event_loop =
        calloop::EventLoop::<AppData>::try_new().expect("create Wayland event loop");
    let qh = event_queue.handle();
    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .expect("insert Wayland event source");

    event_loop
        .handle()
        .insert_source(rx, |event, (), state| match event {
            calloop::channel::Event::Msg(WaylandRequest::Activate(handle)) => {
                if let Some(seat) = state.seat_state.seats().next()
                    && let Some(toplevel) = state.cosmic_toplevel(&handle)
                {
                    state
                        .toplevel_manager_state
                        .manager
                        .activate(&toplevel, &seat);
                }
            }
            calloop::channel::Event::Closed => state.exit = true,
        })
        .expect("insert request channel");

    let registry_state = RegistryState::new(&globals);
    let mut state = AppData {
        exit: false,
        tx,
        seat_state: SeatState::new(&globals, &qh),
        toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
        toplevel_manager_state: ToplevelManagerState::new(&registry_state, &qh),
        registry_state,
    };
    while !state.exit {
        event_loop
            .dispatch(None, &mut state)
            .expect("dispatch Wayland event");
    }
}

sctk::delegate_seat!(AppData);
sctk::delegate_registry!(AppData);
cctk::delegate_toplevel_info!(AppData);
cctk::delegate_toplevel_manager!(AppData);
