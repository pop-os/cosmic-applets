// Copyright 2025 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use calloop::channel::*;
use cctk::{
    sctk::{
        self,
        reexports::{
            calloop::{self, channel},
            calloop_wayland_source::WaylandSource,
        },
        registry::RegistryState,
    },
    wayland_client::{self, globals::GlobalListContents, protocol::wl_registry, Dispatch, Proxy},
};
use cosmic::iced::futures::{self, SinkExt};
use cosmic_protocols::a11y::v1::client::cosmic_a11y_manager_v1;
use futures::{channel::mpsc, executor::block_on};
use wayland_client::{globals::registry_queue_init, Connection};

use super::{AccessibilityEvent, AccessibilityRequest};

pub fn spawn_a11y(
    tx: mpsc::Sender<AccessibilityEvent>,
) -> anyhow::Result<SyncSender<AccessibilityRequest>> {
    let (a11y_tx, a11y_rx) = calloop::channel::sync_channel(100);
    let conn = Connection::connect_to_env()?;

    std::thread::spawn(move || {
        struct State {
            loop_signal: calloop::LoopSignal,
            tx: mpsc::Sender<AccessibilityEvent>,
            global: cosmic_a11y_manager_v1::CosmicA11yManagerV1,

            magnifier: bool,
        }

        impl Dispatch<cosmic_a11y_manager_v1::CosmicA11yManagerV1, ()> for State {
            fn event(
                state: &mut Self,
                _proxy: &cosmic_a11y_manager_v1::CosmicA11yManagerV1,
                event: <cosmic_a11y_manager_v1::CosmicA11yManagerV1 as Proxy>::Event,
                _data: &(),
                _conn: &Connection,
                _qhandle: &sctk::reexports::client::QueueHandle<Self>,
            ) {
                match event {
                    cosmic_a11y_manager_v1::Event::Magnifier { active } => {
                        let magnifier = active
                            .into_result()
                            .unwrap_or(cosmic_a11y_manager_v1::ActiveState::Disabled)
                            == cosmic_a11y_manager_v1::ActiveState::Enabled;
                        if magnifier != state.magnifier {
                            if block_on(state.tx.send(AccessibilityEvent::Magnifier(magnifier)))
                                .is_err()
                            {
                                state.loop_signal.stop();
                                state.loop_signal.wakeup();
                            };
                            state.magnifier = magnifier;
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
            fn event(
                _state: &mut Self,
                _proxy: &wl_registry::WlRegistry,
                _event: <wl_registry::WlRegistry as Proxy>::Event,
                _data: &GlobalListContents,
                _conn: &Connection,
                _qhandle: &sctk::reexports::client::QueueHandle<Self>,
            ) {
                // We don't care about any dynamic globals
            }
        }

        let mut event_loop = calloop::EventLoop::<State>::try_new().unwrap();

        let loop_handle = event_loop.handle();
        let (globals, event_queue) = registry_queue_init(&conn).unwrap();
        let qhandle = event_queue.handle();

        WaylandSource::new(conn, event_queue)
            .insert(loop_handle.clone())
            .unwrap();

        let registry_state = RegistryState::new(&globals);
        let global = registry_state
            .bind_one::<cosmic_a11y_manager_v1::CosmicA11yManagerV1, _, _>(&qhandle, 1..=1, ())
            .unwrap();

        loop_handle
            .insert_source(a11y_rx, |request, _, state| match request {
                channel::Event::Msg(AccessibilityRequest::Magnifier(val)) => {
                    state.global.set_magnifier(if val {
                        cosmic_a11y_manager_v1::ActiveState::Enabled
                    } else {
                        cosmic_a11y_manager_v1::ActiveState::Disabled
                    });
                }
                channel::Event::Closed => {
                    state.loop_signal.stop();
                    state.loop_signal.wakeup();
                }
            })
            .unwrap();

        let mut state = State {
            loop_signal: event_loop.get_signal(),
            tx,
            global,

            magnifier: false,
        };

        event_loop.run(None, &mut state, |_| {}).unwrap();
    });

    Ok(a11y_tx)
}
