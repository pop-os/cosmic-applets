use gtk4::{glib::Sender, prelude::*, Button, Image, Label, ListBox};
use mpris2_zbus::{media_player::MediaPlayer, metadata::Metadata};
use std::time::Duration;
use tokio::time::sleep;
use zbus::Connection;

pub async fn metadata_update(tx: Sender<Vec<Metadata>>) {
    let connection = Connection::session()
        .await
        .expect("failed to connect to zbus");
    loop {
        sleep(Duration::from_secs(1)).await;
        let media_players = MediaPlayer::new_all(&connection)
            .await
            .expect("failed to get media players");
    }
}
