use crate::wayland::{self, WorkspaceEvent, WorkspaceList};
use cctk::sctk::reexports::calloop::channel::SyncSender;
use cosmic::iced::{
    self,
    futures::{channel::mpsc, SinkExt, StreamExt},
    subscription,
};
use std::hash::Hash;

#[derive(Debug, Clone)]
pub enum WorkspacesUpdate {
    Workspaces(WorkspaceList),
    Started(SyncSender<WorkspaceEvent>),
    Errored,
}

pub fn workspaces<I: 'static + Hash + Copy + Send + Sync>(
    id: I,
) -> iced::Subscription<WorkspacesUpdate> {
    subscription::channel(id, 50, move |mut output| async move {
        let mut state = State::Ready;

        loop {
            state = start_listening(state, &mut output).await;
        }
    })
}

async fn start_listening(
    state: State,
    output: &mut futures::channel::mpsc::Sender<WorkspacesUpdate>,
) -> State {
    match state {
        State::Ready => {
            if let Ok(watcher) = WorkspacesWatcher::new() {
                _ = output
                    .send(WorkspacesUpdate::Started(watcher.get_sender()))
                    .await;
                State::Waiting(watcher)
            } else {
                _ = output.send(WorkspacesUpdate::Errored).await;

                State::Error
            }
        }
        State::Waiting(mut t) => {
            if let Some(w) = t.workspaces().await {
                _ = output.send(WorkspacesUpdate::Workspaces(w)).await;
                State::Waiting(t)
            } else {
                _ = output.send(WorkspacesUpdate::Errored).await;
                State::Error
            }
        }
        State::Error => cosmic::iced::futures::future::pending().await,
    }
}

pub enum State {
    Ready,
    Waiting(WorkspacesWatcher),
    Error,
}

pub struct WorkspacesWatcher {
    rx: mpsc::Receiver<WorkspaceList>,
    tx: SyncSender<WorkspaceEvent>,
}

impl WorkspacesWatcher {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(20);
        let tx = wayland::spawn_workspaces(tx);
        Ok(Self { tx, rx })
    }

    pub fn get_sender(&self) -> SyncSender<WorkspaceEvent> {
        self.tx.clone()
    }

    pub async fn workspaces(&mut self) -> Option<WorkspaceList> {
        self.rx.next().await
    }
}
