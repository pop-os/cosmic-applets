use std::{any::TypeId, io, process::Stdio, time::Duration};

use cosmic::iced_futures::futures::future;
use swaybar_types::Block;
use tokio::{
    io::{AsyncBufReadExt, BufReader, Lines},
    process::{Child, ChildStdout, Command},
    time::sleep,
};

// TODO: support auto-detection of `i3status` and `i3status-rs` executables
// TODO: support basename-only commands by searching PATH (using "which" crate?)
// TODO: provide GUI for user to add their own preferred command
const COMMAND: &str = "/usr/bin/i3status-rs";

fn spawn() -> io::Result<Child> {
    Command::new(COMMAND)
        // `.stdin(Stdio::null())` is faster, but `i3status-rs` requires a working stdio
        // (and the i3bar protocol doesn't specify either way)
        // TODO: support click events on blocks
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
}

async fn read_blocks(state: State) -> (Output, State) {
    match state {
        State::Ready => match spawn() {
            Ok(mut child) => {
                let stdout = child
                    .stdout
                    .take()
                    .expect("should capture stdout from i3status");
                let reader = BufReader::new(stdout);
                let stdout_lines = reader.lines();
                (
                    Output::Raw(String::from("i3status started")),
                    State::Running {
                        child,
                        stdout_lines,
                    },
                )
            }
            Err(_) => (
                Output::Raw(String::from("cannot spawn i3status")),
                State::Finished,
            ),
        },
        State::Running {
            child,
            mut stdout_lines,
        } => {
            match stdout_lines.next_line().await {
                Ok(Some(line)) => {
                    // for more information about the protocol,
                    // see: https://i3wm.org/docs/i3bar-protocol.html

                    // the "endless array" output means we have a dangling comma to remove
                    let line = line.trim_end_matches(',');

                    if let Ok(blocks) = serde_json::from_str::<Vec<Block>>(line) {
                        (
                            Output::Blocks(blocks),
                            State::Running {
                                child,
                                stdout_lines,
                            },
                        )
                    } else {
                        (
                            Output::Raw(String::from(line)),
                            State::Running {
                                child,
                                stdout_lines,
                            },
                        )
                    }
                }
                Ok(None) => {
                    sleep(Duration::from_secs(3)).await;
                    (
                        Output::None,
                        State::Running {
                            child,
                            stdout_lines,
                        },
                    )
                }
                Err(_) => (
                    Output::Raw(String::from("cannot read i3status stdout")),
                    State::Finished,
                ),
            }
        }
        State::Finished => {
            // We do not let the stream die, as it would start a
            // new download repeatedly if the user is not careful
            // in case of errors.
            future::pending().await
        }
    }
}

pub fn child_process() -> cosmic::iced::Subscription<Output> {
    struct SomeWorker;
    cosmic::iced::subscription::unfold(TypeId::of::<SomeWorker>(), State::Ready, |state| {
        read_blocks(state)
    })
}

#[derive(Clone, Debug)]
pub enum Output {
    Blocks(Vec<Block>),
    Raw(String),
    None,
}

#[derive(Debug)]
pub enum State {
    Ready,
    Running {
        child: Child,
        stdout_lines: Lines<BufReader<ChildStdout>>,
    },
    Finished,
}
