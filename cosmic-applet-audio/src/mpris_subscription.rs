// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::{borrow::Cow, fmt::Debug, hash::Hash, path::PathBuf};

use cosmic::{
    iced::{self, subscription},
    iced_futures::futures::{self, future::OptionFuture, SinkExt, StreamExt},
};
use mpris2_zbus::{
    enumerator,
    media_player::MediaPlayer,
    player::{PlaybackStatus, Player},
};
use tokio::join;
use urlencoding::decode;
use zbus::{
    names::{BusName, OwnedBusName},
    Connection,
};

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
        let pathname = metadata.url().unwrap_or("".into());
        let pathbuf = PathBuf::from(pathname);

        let title = metadata
            .title()
            .or(pathbuf
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| decode(s).map_or(None, |s| Some(s.into_owned()))))
            .map(Cow::from);
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
        run(&mut output).await;
        let _ = output.send(MprisUpdate::Finished).await;
        futures::future::pending().await
    })
}

#[derive(Clone, Debug)]
struct MprisPlayer {
    player: Player,
    #[allow(dead_code)]
    media_player: MediaPlayer,
}

impl MprisPlayer {
    async fn new(conn: &Connection, name: OwnedBusName) -> mpris2_zbus::error::Result<Self> {
        Ok(Self {
            player: Player::new(conn, name.clone()).await?,
            media_player: MediaPlayer::new(conn, name).await?,
        })
    }

    fn name(&self) -> &BusName {
        self.player.inner().destination()
    }
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

struct State {
    conn: Connection,
    enumerator_stream:
        Box<dyn futures::Stream<Item = zbus::Result<enumerator::Event>> + Unpin + Send>,
    players: Vec<MprisPlayer>,
    active_player: Option<MprisPlayer>,
    active_player_metadata_stream: Option<Box<dyn futures::Stream<Item = ()> + Unpin + Send>>,
    any_player_state_stream: futures::stream::SelectAll<zbus::PropertyStream<'static, String>>,
}

fn filter_firefox_players(players: &mut Vec<MprisPlayer>) {
    if players
        .iter()
        .any(|e| e.name() == "org.mpris.MediaPlayer2.plasma-browser-integration")
    {
        players.retain(|e| !e.name().starts_with("org.mpris.MediaPlayer2.firefox."));
    }
}

impl State {
    async fn new() -> Result<Self, zbus::Error> {
        let conn = Connection::session().await?;

        let enumerator = enumerator::Enumerator::new(&conn).await?;
        let enumerator_stream = enumerator.receive_changes().await?;

        let player_names = enumerator.players().await?;
        let mut players = Vec::with_capacity(player_names.len());
        for name in player_names {
            match MprisPlayer::new(&conn, name).await {
                Ok(player) => {
                    players.push(player);
                }
                Err(err) => {
                    tracing::error!("Failed to add player: {}", err);
                }
            }
        }
        filter_firefox_players(&mut players);

        // pre-sort by path so that the same player is always selected
        players.sort_by(|a, b| a.name().cmp(&b.name()));

        let mut state = Self {
            conn,
            enumerator_stream: Box::new(enumerator_stream),
            players,
            active_player: None,
            active_player_metadata_stream: None,
            any_player_state_stream: futures::stream::select_all(Vec::new()),
        };
        state.update_active_player().await;
        state.update_any_player_state_stream().await;
        Ok(state)
    }

    async fn add_player(&mut self, name: OwnedBusName) {
        let player = match MprisPlayer::new(&self.conn, name).await {
            Ok(player) => player,
            Err(err) => {
                tracing::error!("Failed to add player: {}", err);
                return;
            }
        };
        self.players.push(player);
        filter_firefox_players(&mut self.players);
        self.players.sort_by(|a, b| a.name().cmp(&b.name()));
        self.update_any_player_state_stream().await;
    }

    async fn remove_player(&mut self, name: OwnedBusName) {
        if let Some(idx) = self.players.iter().position(|p| p.name() == &name) {
            self.players.remove(idx);
        }
        self.update_any_player_state_stream().await;
    }

    async fn update_active_player(&mut self) {
        let new_active_player = find_active(&self.players).await;
        if self.active_player.as_ref().map(|p| p.name()) != new_active_player.map(|p| p.name()) {
            self.active_player = new_active_player.cloned();
            if let Some(player) = new_active_player {
                let controls_changed = futures::stream::select_all([
                    player.player.receive_can_pause_changed().await,
                    player.player.receive_can_play_changed().await,
                    player.player.receive_can_go_previous_changed().await,
                    player.player.receive_can_go_next_changed().await,
                ]);
                let metadata_changed = player.player.receive_metadata_changed().await;
                let stream = futures::stream::select(
                    controls_changed.map(|_| ()),
                    metadata_changed.map(|_| ()),
                );
                self.active_player_metadata_stream = Some(Box::new(stream));
            } else {
                self.active_player_metadata_stream = None;
            }
        }
    }

    async fn update_any_player_state_stream(&mut self) {
        let mut listeners = Vec::with_capacity(self.players.len());
        for p in &self.players {
            listeners.push(p.player.receive_playback_status_changed().await);
        }
        self.any_player_state_stream = futures::stream::select_all(listeners);
    }
}

async fn run(output: &mut futures::channel::mpsc::Sender<MprisUpdate>) {
    let mut state = match State::new().await {
        Ok(state) => state,
        Err(err) => {
            tracing::error!("Faile do monitor for mpris clients: {}", err);
            return;
        }
    };

    loop {
        if let Some(player) = &state.active_player {
            if let Some(player_status) = PlayerStatus::new(player.player.clone()).await {
                _ = output.send(MprisUpdate::Player(player_status)).await;
            } else {
                tracing::error!("Failed to get player status.");
            };
        } else {
            let _ = output.send(MprisUpdate::Setup).await;
        }

        let metadata_changed_next = OptionFuture::from(
            state
                .active_player_metadata_stream
                .as_mut()
                .map(|s| s.next()),
        );
        tokio::select! {
            _ = metadata_changed_next, if state.active_player.is_some() => {
            },
            event = state.enumerator_stream.next() => {
                match event {
                    Some(Ok(enumerator::Event::Add(name))) => state.add_player(name).await,
                    Some(Ok(enumerator::Event::Remove(name))) => state.remove_player(name).await,
                    Some(Err(err)) => {
                        tracing::error!("Error listening for mpris clients: {:?}", err);
                        return;
                    }
                    None => {}
                }
                state.update_active_player().await;
            }
            _ = state.any_player_state_stream.next(), if !state.players.is_empty() => {
                state.update_active_player().await;
            },
        };
    }
}

async fn find_active<'a>(players: &'a Vec<MprisPlayer>) -> Option<&'a MprisPlayer> {
    let mut best = (0, None::<&'a MprisPlayer>);
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

    for p in players.iter() {
        let v = eval(p.player.clone()).await;
        if v > best.0 {
            best = (v, Some(p));
        }
    }

    best.1
}
