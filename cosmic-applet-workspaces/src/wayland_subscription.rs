use crate::wayland::{self, WorkspaceEvent, WorkspaceList};
use cctk::sctk::reexports::calloop::channel::SyncSender;
use cosmic::iced::{
    self,
    futures::{channel::mpsc, SinkExt, StreamExt},
    subscription,
};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

pub static WAYLAND_RX: Lazy<Mutex<Option<mpsc::Receiver<WorkspaceList>>>> =
    Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone)]
pub enum WorkspacesUpdate {
    Workspaces(WorkspaceList),
    Started(SyncSender<WorkspaceEvent>),
    Errored,
}

pub fn workspaces() -> iced::Subscription<WorkspacesUpdate> {
    subscription::channel(
        std::any::TypeId::of::<WorkspacesUpdate>(),
        50,
        move |mut output| async move {
            let mut state = State::Waiting;

            loop {
                state = start_listening(state, &mut output).await;
            }
        },
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
                _ = output.send(WorkspacesUpdate::Workspaces(w)).await;
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
    rx: mpsc::Receiver<WorkspaceList>,
    tx: SyncSender<WorkspaceEvent>,
}

impl WorkspacesWatcher {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(20);
        let tx = wayland::spawn_workspaces(tx);
        Ok(Self { tx, rx })
    }
}
