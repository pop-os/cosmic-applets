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
use zbus::{fdo::DBusProxy, Connection};

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
    async fn new(player: Player) -> Option<Self> {
        let metadata = player.metadata().await.ok()?;
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
        Some(Self {
            icon,
            title,
            artists,
            status: playback_status.unwrap_or(PlaybackStatus::Stopped),
            can_pause: can_pause.unwrap_or_default(),
            can_play: can_play.unwrap_or_default(),
            can_go_previous: can_go_previous.unwrap_or_default(),
            can_go_next: can_go_next.unwrap_or_default(),
            player,
        })
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
    Player(Player, DBusProxy<'static>),
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
                _ = output.send(MprisUpdate::Finished).await;
                return State::Finished;
            };
            let mut players = mpris2_zbus::media_player::MediaPlayer::new_all(&conn)
                .await
                .unwrap_or_else(|_| Vec::new());
            let Ok(dbus_proxy) = zbus::fdo::DBusProxy::builder(&conn)
                .path("/org/freedesktop/DBus")
                .unwrap()
                .build()
                .await
            else {
                tracing::error!("Failed to create dbus proxy.");
                return State::Finished;
            };
            if players.is_empty() {
                let Ok(mut stream) = dbus_proxy.receive_name_owner_changed().await else {
                    tracing::error!("Failed to receive name owner changed signal.");
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    // restart from the beginning
                    return State::Setup;
                };
                while let Some(c) = stream.next().await {
                    if let Ok(args) = c.args() {
                        if args.name.contains("org.mpris.MediaPlayer2") {
                            if let Ok(p) =
                                MediaPlayer::new(&conn, args.name().to_owned().into()).await
                            {
                                players.push(p);
                            }
                            break;
                        }
                    }
                }
            }

            let Some(player) = find_active(players).await else {
                tracing::error!("Failed to find active media player.");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                return State::Setup;
            };

            let Some(player_status) = PlayerStatus::new(player.clone()).await else {
                tracing::error!("Failed to get player status.");
                return State::Setup;
            };

            _ = output.send(MprisUpdate::Player(player_status)).await;
            State::Player(player, dbus_proxy)
        }
        State::Player(player, dbus_proxy) => {
            let Ok(mut name_owner_changed) = player.receive_owner_changed().await else {
                tracing::error!("Failed to receive owner changed signal.");
                // restart from the beginning
                return State::Setup;
            };
            let mut metadata_changed = player.receive_metadata_changed().await;
            let Ok(mut new_mpris) = dbus_proxy.receive_name_owner_changed().await else {
                tracing::error!("Failed to receive name owner changed signal.");
                // restart from the beginning
                return State::Setup;
            };
            let conn = player.connection();
            let media_players = mpris2_zbus::media_player::MediaPlayer::new_all(&conn)
                .await
                .unwrap_or_else(|_| Vec::new());

            let mut players = Vec::with_capacity(media_players.len());
            for p in media_players {
                if let Ok(p) = p.player().await {
                    players.push(p);
                }
            }

            loop {
                let mut listeners = Vec::with_capacity(players.len());
                for p in &players {
                    listeners.push(p.receive_playback_status_changed().await);
                }
                let mut player_state_changed_list = Vec::with_capacity(listeners.len());
                for l in &mut listeners {
                    player_state_changed_list.push(Box::pin(async move {
                        let changed = l.next().await;
                        if let Some(c) = changed {
                            c.get().await.ok()
                        } else {
                            tracing::error!("Failed to receive playback status changed signal.");
                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                            None
                        }
                    }));
                }
                let any_player_state_changed =
                    futures::future::select_all(player_state_changed_list);
                let keep_going = tokio::select! {
                    m = metadata_changed.next() => {
                        m.is_some()
                    },
                    n = name_owner_changed.next() => {
                        n.map(|n| n.is_some()).unwrap_or_default()
                    },
                    _ = new_mpris.next() => {
                        true
                    },
                    _ = any_player_state_changed => {
                        true
                    },
                };

                if !keep_going {
                    break;
                }

                if let Some(update) = PlayerStatus::new(player.clone()).await {
                    if matches!(update.status, PlaybackStatus::Stopped) {
                        break;
                    }

                    // if paused check if any players are playing
                    // if they are, break
                    if !matches!(update.status, PlaybackStatus::Playing) {
                        let conn = player.connection();
                        let players = mpris2_zbus::media_player::MediaPlayer::new_all(&conn)
                            .await
                            .unwrap_or_else(|_| Vec::new());
                        if let Some(active) = find_active(players).await {
                            if active.destination() != player.destination() {
                                break;
                            }
                        }
                    }
                    _ = output.send(MprisUpdate::Player(update)).await;
                } else {
                    break;
                }
            }
            _ = output.send(MprisUpdate::Setup).await;
            State::Setup
        }
        State::Finished => iced::futures::future::pending().await,
    }
}

async fn find_active(mut players: Vec<MediaPlayer>) -> Option<Player> {
    // pre-sort by path so that the same player is always selected
    players.sort_by(|a, b| {
        let a = a.destination();
        let b = b.destination();
        a.cmp(&b)
    });
    let mut best = (0, None);
    let eval = |p: Player| async move {
        let v = {
            let status = p.playback_status().await;

            match status {
                Ok(mpris2_zbus::player::PlaybackStatus::Playing) => 100,
                Ok(mpris2_zbus::player::PlaybackStatus::Paused) => 10,
                _ => return 0,
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
        if v > best.0 {
            best = (v, Some(p));
        }
    }

    best.1
}
