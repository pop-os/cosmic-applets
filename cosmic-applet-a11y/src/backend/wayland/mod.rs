// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use anyhow;
use cctk::sctk::reexports::calloop::channel::SyncSender;
use cosmic::iced::{
    self,
    futures::{self, channel::mpsc, SinkExt, StreamExt},
    stream, Subscription,
};
use cosmic_protocols::a11y::v1::client::cosmic_a11y_manager_v1::Filter;
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

mod thread;

pub static WAYLAND_RX: Lazy<Mutex<Option<mpsc::Receiver<AccessibilityEvent>>>> =
    Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone)]
pub enum WaylandUpdate {
    State(AccessibilityEvent),
    Started(SyncSender<AccessibilityRequest>),
    Errored,
}

#[derive(Debug, Clone, Copy)]
pub enum AccessibilityEvent {
    Bound(u32),
    Magnifier(bool),
    ScreenFilter { inverted: bool, filter: Filter },
}

#[derive(Debug, Clone, Copy)]
pub enum AccessibilityRequest {
    Magnifier(bool),
    ScreenFilter { inverted: bool, filter: Filter },
}

pub fn a11y_subscription() -> iced::Subscription<WaylandUpdate> {
    Subscription::run_with_id(
        std::any::TypeId::of::<WaylandUpdate>(),
        stream::channel(50, move |mut output| async move {
            let mut state = State::Waiting;

            loop {
                state = start_listening(state, &mut output).await;
            }
        }),
    )
}

async fn start_listening(
    state: State,
    output: &mut futures::channel::mpsc::Sender<WaylandUpdate>,
) -> State {
    match state {
        State::Waiting => {
            let mut guard = WAYLAND_RX.lock().await;
            let rx = {
                if guard.is_none() {
                    if let Ok(WaylandWatcher { rx, tx }) = WaylandWatcher::new() {
                        *guard = Some(rx);
                        _ = output.send(WaylandUpdate::Started(tx)).await;
                    } else {
                        _ = output.send(WaylandUpdate::Errored).await;
                        return State::Error;
                    }
                }
                guard.as_mut().unwrap()
            };
            if let Some(w) = rx.next().await {
                _ = output.send(WaylandUpdate::State(w)).await;
                State::Waiting
            } else {
                _ = output.send(WaylandUpdate::Errored).await;
                State::Error
            }
        }
        State::Error => cosmic::iced::futures::future::pending().await,
    }
}

pub enum State {
    Waiting,
    Error,
}

pub struct WaylandWatcher {
    rx: mpsc::Receiver<AccessibilityEvent>,
    tx: SyncSender<AccessibilityRequest>,
}

impl WaylandWatcher {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(20);
        let tx = thread::spawn_a11y(tx)?;
        Ok(Self { tx, rx })
    }
}
