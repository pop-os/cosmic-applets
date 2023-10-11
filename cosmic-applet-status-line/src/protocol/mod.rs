/// TODO: if we get an error, terminate process with exit code 1. Let cosmic-panel restart us.
/// TODO: configuration for command? Use cosmic config system.
use cosmic::iced::{self, futures::FutureExt};
use std::{
    fmt,
    io::{BufRead, BufReader},
    process::{self, Stdio},
    thread,
};
use tokio::sync::mpsc;

mod serialization;
use serialization::Header;
pub use serialization::{Block, ClickEvent};

#[derive(Clone, Debug, Default)]
pub struct StatusLine {
    pub blocks: Vec<Block>,
    pub click_events: bool,
}

pub fn subscription() -> iced::Subscription<StatusLine> {
    iced::subscription::run_with_id(
        "status-cmd",
        async {
            let (sender, reciever) = mpsc::channel(20);
            thread::spawn(move || {
                let mut status_cmd = StatusCmd::spawn();
                let mut deserializer =
                    serde_json::Deserializer::from_reader(&mut status_cmd.stdout);
                deserialize_status_lines(&mut deserializer, |blocks| {
                    sender
                        .blocking_send(StatusLine {
                            blocks,
                            click_events: status_cmd.header.click_events,
                        })
                        .unwrap();
                })
                .unwrap();
                status_cmd.wait();
            });
            tokio_stream::wrappers::ReceiverStream::new(reciever)
        }
        .flatten_stream(),
    )
}

pub struct StatusCmd {
    header: Header,
    stdin: process::ChildStdin,
    stdout: BufReader<process::ChildStdout>,
    child: process::Child,
}

impl StatusCmd {
    fn spawn() -> StatusCmd {
        // XXX command
        // XXX unwrap
        let mut child = process::Command::new("i3status")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut header = String::new();
        stdout.read_line(&mut header).unwrap();

        StatusCmd {
            header: serde_json::from_str(&header).unwrap(),
            stdin: child.stdin.take().unwrap(),
            stdout,
            child,
        }
    }

    fn wait(mut self) {
        drop(self.stdin);
        drop(self.stdout);
        self.child.wait();
    }
}

/// Deserialize a sequence of `Vec<Block>`s, executing a callback for each one.
/// Blocks thread until end of status line sequence.
fn deserialize_status_lines<'de, D: serde::Deserializer<'de>, F: FnMut(Vec<Block>)>(
    deserializer: D,
    cb: F,
) -> Result<(), D::Error> {
    struct Visitor<F: FnMut(Vec<Block>)> {
        cb: F,
    }

    impl<'de, F: FnMut(Vec<Block>)> serde::de::Visitor<'de> for Visitor<F> {
        type Value = ();

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a sequence of status lines")
        }

        fn visit_seq<S: serde::de::SeqAccess<'de>>(mut self, mut seq: S) -> Result<(), S::Error> {
            while let Some(blocks) = seq.next_element()? {
                (self.cb)(blocks);
            }
            Ok(())
        }
    }

    let visitor = Visitor { cb };
    deserializer.deserialize_seq(visitor)
}
