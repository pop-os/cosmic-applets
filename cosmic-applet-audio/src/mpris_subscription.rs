use std::{borrow::Cow, fmt::Debug, hash::Hash, path::PathBuf};

use cosmic::{
    iced::{self, subscription},
    iced_futures::futures::{self, SinkExt, StreamExt},
};
use mpris2_zbus::{
    media_player::MediaPlayer,
    player::{PlaybackStatus, Player},
};
use tokio::join;
use zbus::Connection;

#[derive(Clone, Debug)]
pub struct PlayerStatus {
    pub player: Player,
    pub icon: Option<PathBuf>,
    pub title: Option<Cow<'static, str>>,
    pub artists: Option<Vec<Cow<'static, str>>>,
    pub status: PlaybackStatus,
    pub can_pause: bool,
    pub can_play: bool,
    pub can_go_previous: bool,
    pub can_go_next: bool,
}

impl PlayerStatus {
    async fn new(player: Player) -> Self {
        let metadata = player.metadata().await.unwrap();
        let title = metadata.title().map(Cow::from);
        let artists = metadata
            .artists()
            .map(|a| a.into_iter().map(Cow::from).collect::<Vec<_>>());
        let icon = metadata
            .art_url()
            .and_then(|u| url::Url::parse(&u).ok())
            .and_then(|u| {
                if u.scheme() == "file" {
                    u.to_file_path().ok()
                } else {
                    None
                }
            });

        let (playback_status, can_pause, can_play, can_go_previous, can_go_next) = join!(
            player.playback_status(),
            player.can_pause(),
            player.can_play(),
            player.can_go_previous(),
            player.can_go_next()
        );
        Self {
            icon,
            title,
            artists,
            status: playback_status.unwrap_or(PlaybackStatus::Stopped),
            can_pause: can_pause.unwrap_or_default(),
            can_play: can_play.unwrap_or_default(),
            can_go_previous: can_go_previous.unwrap_or_default(),
            can_go_next: can_go_next.unwrap_or_default(),
            player,
        }
    }
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
    Player(Player),
    Finished,
}

#[derive(Clone, Debug)]
pub enum MprisUpdate {
    Setup,
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
            let Ok(conn) = Connection::session().await else {
                tracing::error!("Failed to connect to session bus.");
                return State::Finished;
            };
            let mut players = mpris2_zbus::media_player::MediaPlayer::new_all(&conn)
                .await
                .unwrap_or_else(|_| Vec::new());
            if players.is_empty() {
                let Ok(dbus) = zbus::fdo::DBusProxy::builder(&conn)
                    .path("/org/freedesktop/DBus")
                    .unwrap()
                    .build()
                    .await
                else {
                    tracing::error!("Failed to create dbus proxy.");
                    return State::Finished;
                };
                let Ok(mut stream) = dbus.receive_name_owner_changed().await else {
                    tracing::error!("Failed to receive name owner changed signal.");
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    // restart from the beginning
                    return State::Setup;
                };
                while let Some(c) = stream.next().await {
                    if let Ok(args) = c.args() {
                        if args.name.contains("org.mpris.MediaPlayer2") {
                            break;
                        }
                    }
                }
                if let Ok(p) = mpris2_zbus::media_player::MediaPlayer::new_all(&conn).await {
                    players = p;
                } else {
                    // restart from the beginning
                    return State::Setup;
                }
            }

            let Some(player) = find_active(players).await else {
                tracing::error!("Failed to find active media player.");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                return State::Finished;
            };

            let player_status = PlayerStatus::new(player.clone()).await;

            _ = output.send(MprisUpdate::Player(player_status)).await;
            State::Player(player)
        }
        State::Player(player) => {
            let mut paused = player.receive_playback_status_changed().await;
            let mut metadata_changed = player.receive_metadata_changed().await;
            loop {
                let keep_going = tokio::select! {
                    p = paused.next() => {
                        p.is_some()
                    },
                    m = metadata_changed.next() => {
                        m.is_some()
                    },
                };

                if keep_going {
                    let update = PlayerStatus::new(player.clone()).await;
                    let stopped = update.status == PlaybackStatus::Stopped;
                    _ = output.send(MprisUpdate::Player(update)).await;
                    if stopped {
                        _ = output.send(MprisUpdate::Setup).await;
                        break;
                    }
                } else {
                    break;
                }
            }
            State::Setup
        }
        State::Finished => iced::futures::future::pending().await,
    }
}

async fn find_active(players: Vec<MediaPlayer>) -> Option<Player> {
    let mut best = (0, None);
    let eval = |p: Player| async move {
        let v = {
            let status = p.playback_status().await;

            match status {
                Ok(mpris2_zbus::player::PlaybackStatus::Playing) => 100,
                Ok(mpris2_zbus::player::PlaybackStatus::Paused) => 10,
                _ => 0,
            }
        };

        v + p.metadata().await.is_ok() as i32
    };

    for p in players {
        let p = match p.player().await {
            Ok(p) => p,
            Err(_) => continue,
        };
        let v = eval(p.clone()).await;
        if v >= best.0 {
            best = (v, Some(p));
        }
    }

    best.1
}
