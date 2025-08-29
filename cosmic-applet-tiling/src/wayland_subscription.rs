// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::wayland::{self, AppRequest};
use cctk::sctk::reexports::calloop::channel::SyncSender;
use cosmic::iced::{
    self,
    futures::{self, channel::mpsc, SinkExt, StreamExt},
    stream, Subscription,
};
use cosmic_protocols::workspace::v2::client::zcosmic_workspace_handle_v2::TilingState;
use std::sync::LazyLock;
use tokio::sync::Mutex;

pub static WAYLAND_RX: LazyLock<Mutex<Option<mpsc::Receiver<TilingState>>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone)]
pub enum WorkspacesUpdate {
    State(TilingState),
    Started(SyncSender<AppRequest>),
    Errored,
}

pub fn workspaces() -> iced::Subscription<WorkspacesUpdate> {
    Subscription::run_with_id(
        std::any::TypeId::of::<WorkspacesUpdate>(),
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
    output: &mut futures::channel::mpsc::Sender<WorkspacesUpdate>,
) -> State {
    match state {
        State::Waiting => {
            let mut guard = WAYLAND_RX.lock().await;
            let rx = {
                if guard.is_none() {
                    if let Ok(WorkspacesWatcher { rx, tx }) = WorkspacesWatcher::new() {
                        *guard = Some(rx);
                        _ = output.send(WorkspacesUpdate::Started(tx)).await;
                    } else {
                        _ = output.send(WorkspacesUpdate::Errored).await;
                        return State::Error;
                    }
                }
                guard.as_mut().unwrap()
            };
            if let Some(w) = rx.next().await {
                _ = output.send(WorkspacesUpdate::State(w)).await;
                State::Waiting
            } else {
                _ = output.send(WorkspacesUpdate::Errored).await;
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

pub struct WorkspacesWatcher {
    rx: mpsc::Receiver<TilingState>,
    tx: SyncSender<AppRequest>,
}

impl WorkspacesWatcher {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(20);
        let tx = wayland::spawn_workspaces(tx);
        Ok(Self { tx, rx })
    }
}
