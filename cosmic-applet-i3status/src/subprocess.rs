use std::{any::TypeId, process::Stdio};

use cosmic::{iced::subscription, iced_futures::futures::SinkExt};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

const CHANNEL_SIZE: usize = 1;

// TODO: support auto-detection of `i3status` and `i3status-rs` executables
// TODO: support basename-only commands by searching PATH (using "which" crate?)
// TODO: provide GUI for user to add their own preferred command
const COMMAND: &str = "/usr/bin/i3status-rs";

pub fn start() -> cosmic::iced::Subscription<Message> {
    struct Worker;
    subscription::channel(
        TypeId::of::<Worker>(),
        CHANNEL_SIZE,
        |mut output| async move {
            loop {
                let mut cmd = Command::new(COMMAND)
                    .args(["--version"])
                    .stdout(Stdio::piped())
                    .spawn()
                    .expect("{command} should start");
                let stdout = cmd
                    .stdout
                    .as_mut()
                    .expect("should capture stdout from {command}");
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Some(line) = lines
                    .next_line()
                    .await
                    .expect("should read from buffered stdout")
                {
                    output
                        .send(Message::Output(line))
                        .await
                        .expect("should send stdout line");
                }

                // effectively end the subscription by never sending more values
                cosmic::iced::futures::future::pending().await
            }
        },
    )
}

#[derive(Clone, Debug)]
pub enum Message {
    Output(String),
    Error(String),
}

enum State {
    Starting,
    Ready(),
}
