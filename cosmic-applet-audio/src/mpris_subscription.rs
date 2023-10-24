use std::{borrow::Cow, fmt::Debug, hash::Hash, path::PathBuf, time::Duration};

use cosmic::{
    iced::{self, subscription},
    iced_futures::futures::{
        self,
        channel::mpsc::{channel, Receiver, Sender},
        SinkExt, StreamExt,
    },
};
use mpris::{PlaybackStatus, PlayerFinder};

#[derive(Clone, Debug)]
pub struct PlayerStatus {
    pub icon: Option<PathBuf>,
    pub title: Option<Cow<'static, str>>,
    pub artists: Option<Vec<Cow<'static, str>>>,
    pub status: PlaybackStatus,
    pub can_pause: bool,
    pub can_play: bool,
    pub can_go_previous: bool,
    pub can_go_next: bool,
}

pub fn mpris_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced::Subscription<MprisUpdate> {
    subscription::channel(id, 50, move |mut output| async move {
        let mut state = State::Setup;

        loop {
            state = update(state, &mut output).await;
        }
    })
}

#[derive(Debug)]
pub enum State {
    Setup,
    Wait(Receiver<MprisUpdate>),
    Finished,
}

#[derive(Clone, Debug)]
pub enum MprisUpdate {
    Setup(Sender<MprisRequest>),
    Player(PlayerStatus),
    Finished,
}

#[derive(Clone, Debug)]
pub enum MprisRequest {
    Play,
    Pause,
    Next,
    Previous,
}

async fn update(state: State, output: &mut futures::channel::mpsc::Sender<MprisUpdate>) -> State {
    match state {
        State::Setup => {
            let (mut tx, rx) = channel(30);
            let (thread_tx, mut thread_rx) = channel(30);
            let _ = std::thread::spawn(move || {
                let mut ctr = 0;
                loop {
                    let player = match PlayerFinder::new().and_then(|f| {
                        f.find_active()
                            .map_err(|e| mpris::DBusError::Miscellaneous(e.to_string()))
                    }) {
                        Ok(p) => {
                            ctr = 0;
                            p
                        }
                        Err(e) => {
                            tracing::error!(?e, "Failed to find active media player.");
                            std::thread::sleep(Duration::from_millis(ctr.min(20) * 100));
                            continue;
                        }
                    };
                    let can_go_next = player.can_go_next().unwrap_or_default();
                    let can_go_previous = player.can_go_previous().unwrap_or_default();
                    let can_play = player.can_play().unwrap_or_default();
                    let can_pause = player.can_pause().unwrap_or_default();

                    let Ok(mut tracker) = player.track_progress(200) else {
                        tracing::error!("Failed to track progress.");
                        std::thread::sleep(Duration::from_secs(2));
                        continue;
                    };
                    let (title, artists, icon) = player
                        .get_metadata()
                        .map(|m| {
                            (
                                m.title().map(|c| Cow::Owned(String::from(c))),
                                m.artists().map(|a| {
                                    a.into_iter()
                                        .map(|a| Cow::from(String::from(a)))
                                        .collect::<Vec<_>>()
                                }),
                                m.art_url()
                                    .and_then(|u| url::Url::parse(u).ok())
                                    .and_then(|u| {
                                        if u.scheme() == "file" {
                                            u.to_file_path().ok()
                                        } else {
                                            None
                                        }
                                    }),
                            )
                        })
                        .unwrap_or_default();
                    if let Err(err) = tx.try_send(MprisUpdate::Player(PlayerStatus {
                        icon,
                        title,
                        artists,
                        status: player
                            .get_playback_status()
                            .unwrap_or(PlaybackStatus::Stopped),
                        can_pause,
                        can_play,
                        can_go_previous,
                        can_go_next,
                    })) {
                        tracing::error!(?err, "Failed to send player update.");
                    }
                    loop {
                        if let Ok(req) = thread_rx.try_next() {
                            match req {
                                Some(MprisRequest::Play) => {
                                    let _ = player.play();
                                }
                                Some(MprisRequest::Pause) => {
                                    let _ = player.pause();
                                }
                                Some(MprisRequest::Next) => {
                                    let _ = player.next();
                                }
                                Some(MprisRequest::Previous) => {
                                    let _ = player.previous();
                                }
                                None => {
                                    return;
                                }
                            }
                        }
                        let tick = tracker.tick();
                        if tick.player_quit {
                            tracing::info!("Player quit.");
                            break;
                        }
                        if tick.progress_changed {
                            let metadata = tick.progress.metadata();
                            if let Err(err) = tx.try_send(MprisUpdate::Player(PlayerStatus {
                                icon: metadata
                                    .art_url()
                                    .and_then(|u| url::Url::parse(u).ok())
                                    .and_then(|u| {
                                        if u.scheme() == "file" {
                                            u.to_file_path().ok()
                                        } else {
                                            None
                                        }
                                    }),
                                title: metadata.title().map(|t| Cow::from(t.to_string())),
                                artists: metadata.artists().map(|a| {
                                    a.into_iter().map(|a| Cow::from(a.to_string())).collect()
                                }),
                                status: tick.progress.playback_status(),
                                can_pause: player.can_pause().unwrap_or_default(),
                                can_play: player.can_play().unwrap_or_default(),
                                can_go_previous: player.can_go_previous().unwrap_or_default(),
                                can_go_next: player.can_go_next().unwrap_or_default(),
                            })) {
                                tracing::error!(?err, "Failed to send player update.");
                                break;
                            }
                        }
                    }
                    drop(tracker);
                }
            });

            let _ = output.send(MprisUpdate::Setup(thread_tx)).await;

            State::Wait(rx)
        }
        State::Wait(mut rx) => match rx.next().await {
            Some(u) => {
                match u {
                    MprisUpdate::Setup(_) => {}
                    u => {
                        let _ = output.send(u).await;
                    }
                }
                State::Wait(rx)
            }
            None => {
                _ = output.send(MprisUpdate::Finished).await;
                return State::Finished;
            }
        },
        State::Finished => iced::futures::future::pending().await,
    }
}
