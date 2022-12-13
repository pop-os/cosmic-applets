use cctk::{
    sctk::{self, event_loop::WaylandSource},
    toplevel_info::{ToplevelInfoHandler, ToplevelInfoState},
    wayland_client,
};
use cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1;
use futures::channel::mpsc::UnboundedSender;
use sctk::registry::{ProvidesRegistryState, RegistryState};
use wayland_client::{globals::registry_queue_init, Connection, QueueHandle};

use crate::toplevel_subscription::{ToplevelRequest, ToplevelUpdate};

struct AppData {
    exit: bool,
    tx: UnboundedSender<ToplevelUpdate>,
    registry_state: RegistryState,
    toplevel_info_state: ToplevelInfoState,
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers!();
}

impl ToplevelInfoHandler for AppData {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info_state
    }

    fn new_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    ) {
        if let Some(info) = self.toplevel_info_state.info(toplevel) {
            let _ = self
                .tx
                .unbounded_send(ToplevelUpdate::AddToplevel(toplevel.clone(), info.clone()));
        }
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    ) {
        if let Some(info) = self.toplevel_info_state.info(toplevel) {
            let _ = self.tx.unbounded_send(ToplevelUpdate::UpdateToplevel(
                toplevel.clone(),
                info.clone(),
            ));
        }
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    ) {
        let _ = self
            .tx
            .unbounded_send(ToplevelUpdate::RemoveToplevel(toplevel.clone()));
    }
}

pub(crate) fn toplevel_handler(
    tx: UnboundedSender<ToplevelUpdate>,
    rx: calloop::channel::Channel<ToplevelRequest>,
) {
    let conn = Connection::connect_to_env().unwrap();
    let (globals, event_queue) = registry_queue_init(&conn).unwrap();
    let mut event_loop = calloop::EventLoop::<AppData>::try_new().unwrap();
    let qh = event_queue.handle();
    let wayland_source = WaylandSource::new(event_queue).unwrap();
    let handle = event_loop.handle();

    if handle
        .insert_source(wayland_source, |_, q, state| q.dispatch_pending(state))
        .is_err()
    {
        return;
    };

    if handle
        .insert_source(rx, |event, _, state| match event {
            calloop::channel::Event::Msg(req) => match req {
                ToplevelRequest::Activate(_) => {} // TODO
                ToplevelRequest::Quit(_) => {}     // TODO
                ToplevelRequest::Exit => {
                    state.exit = true;
                }
            },
            calloop::channel::Event::Closed => {
                state.exit = true;
            }
        })
        .is_err()
    {
        return;
    }
    let registry_state = RegistryState::new(&globals);
    let mut app_data = AppData {
        exit: false,
        tx,
        toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
        registry_state,
    };

    loop {
        if app_data.exit {
            break;
        }
        event_loop.dispatch(None, &mut app_data).unwrap();
    }
}

sctk::delegate_registry!(AppData);
cctk::delegate_toplevel_info!(AppData);
