use crate::wayland::{self, WorkspaceEvent, WorkspaceList};
use calloop::channel::SyncSender;
use futures::{channel::mpsc, StreamExt};
use std::hash::Hash;

#[derive(Debug, Clone)]
pub enum WorkspacesUpdate {
    Workspaces(WorkspaceList),
    Started(SyncSender<WorkspaceEvent>),
    Errored,
}

pub fn workspaces<I: 'static + Hash + Copy + Send + Sync>(
    id: I,
) -> cosmic::iced::Subscription<(I, WorkspacesUpdate)> {
    use cosmic::iced::subscription;

    subscription::unfold(id, State::Ready, move |state| _workspaces(id, state))
}

async fn _workspaces<I: Copy>(id: I, state: State) -> (Option<(I, WorkspacesUpdate)>, State) {
    match state {
        State::Ready => {
            if let Ok(watcher) = WorkspacesWatcher::new() {
                (
                    Some((id, WorkspacesUpdate::Started(watcher.get_sender()))),
                    State::Waiting(watcher),
                )
            } else {
                (Some((id, WorkspacesUpdate::Errored)), State::Error)
            }
        }
        State::Waiting(mut t) => {
            if let Some(w) = t.workspaces().await {
                (
                    Some((id, WorkspacesUpdate::Workspaces(w))),
                    State::Waiting(t),
                )
            } else {
                (Some((id, WorkspacesUpdate::Errored)), State::Error)
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
