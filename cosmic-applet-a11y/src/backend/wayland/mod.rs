// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use anyhow;
use cctk::sctk::reexports::calloop::{self, channel::SyncSender};
use cosmic::iced::{
    self, Subscription,
    futures::{self, SinkExt, StreamExt, channel::mpsc},
    stream,
};
use cosmic_protocols::a11y::v1::client::cosmic_a11y_manager_v1::Filter;
use cosmic_settings_subscriptions::cosmic_a11y_manager::{
    self as thread, AccessibilityEvent, AccessibilityRequest,
};
use std::sync::LazyLock;
use tokio::sync::Mutex;

pub static WAYLAND_RX: LazyLock<Mutex<Option<tokio::sync::mpsc::Receiver<AccessibilityEvent>>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone)]
pub enum WaylandUpdate {
    State(AccessibilityEvent),
    Started(calloop::channel::Sender<AccessibilityRequest>),
    Errored,
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
            if let Some(w) = rx.recv().await {
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
    rx: tokio::sync::mpsc::Receiver<AccessibilityEvent>,
    tx: calloop::channel::Sender<AccessibilityRequest>,
}

impl WaylandWatcher {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = thread::spawn_wayland_connection(1)?;
        Ok(Self { rx, tx })
    }
}
