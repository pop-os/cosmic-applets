use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::UnixStream,
    sync::oneshot,
};
use tracing::{error, info};
use cosmic::{
    iced::{
        futures::{self, SinkExt},
        subscription,
    },
    iced_futures::Subscription,
};
use cosmic_notifications_util::AppletEvent;
use std::os::unix::io::{FromRawFd, RawFd};
use sendfd::RecvWithFd;

#[derive(Debug)]
pub enum State {
    WaitingForPanel,
    WaitingForDaemon(UnixStream),
    WaitingForNotificationEvent(UnixStream),
    Finished,
}

pub fn notifications() -> Subscription<AppletEvent> {
    struct SomeWorker;

    subscription::channel(
        std::any::TypeId::of::<SomeWorker>(),
        50,
        |mut output| async move {
            let mut state = State::WaitingForPanel;

            loop {
                match &mut state {
                    State::WaitingForPanel => {
                        info!("Waiting for panel to send us a stream");

                        let (tx, rx) = oneshot::channel();

                        std::thread::spawn(move || -> anyhow::Result<()> {
                            let mut msg = String::new();
                            std::io::stdin().read_line(&mut msg)?;
                            let raw_fd =  msg.trim().parse::<RawFd>()?;
                            if raw_fd == 0 {
                                anyhow::bail!("Invalid fd received from panel");
                            }
                            _ = tx.send(raw_fd);
                            Ok(())
                        });
                        
                        let Ok(raw_fd) = rx.await else {
                            error!("Failed to receive raw fd from panel");
                            state = State::Finished;
                            continue;
                        };

                        let stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(raw_fd) };
                        let Ok(stream) = UnixStream::from_std(stream) else {
                            error!("Failed to convert std stream to unix stream");
                            state = State::Finished;
                            continue;
                        };
                        state = State::WaitingForDaemon(stream);

                    }
                    State::WaitingForDaemon(stream) => {
                        info!("Waiting for daemon to send us a stream");
                        if let Err(err) = stream.readable().await {
                            error!("Failed to wait for stream to be readable {}", err);
                            state = State::Finished;
                            continue;
                        };

                        // we are expecting a single RawFd from the panel on this stream and the applet id
                        let mut buf = [0u8; 4];
                        let mut fd_buf = [0i32; 1];

                        match stream.recv_with_fd(&mut buf, &mut fd_buf) {
                            Ok((data_cnt, fd_cnt)) => {
                                if data_cnt != 4 || fd_cnt != 1 {
                                    error!("Invalid data received from panel");
                                    state = State::Finished;

                                    continue;
                                }
                                let notif_stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd_buf[0]) };
                                let Ok(notif_stream) = UnixStream::from_std(notif_stream) else {
                                    error!("Failed to convert raw fd to unix stream");
                                    state = State::Finished;
                                    continue;
                                };
                                
                                state = State::WaitingForNotificationEvent(notif_stream);
                            }
                            Err(err) => {
                                error!("Failed to receive fd from panel: {}", err);
                                state = State::Finished;

                                continue;
                            }
                        }
                    }
                    State::WaitingForNotificationEvent(stream) => {
                        let mut reader = BufReader::new(stream);
                        // todo read messages
                        let mut request_buf = String::with_capacity(1024);
                        if let Err(err) = reader.read_line(&mut request_buf).await {
                            error!("Failed to read line from stream {}", err);
                            continue;
                        }
                        let Ok(event) = ron::de::from_str::<AppletEvent>(request_buf.as_str()) else {
                            error!("Failed to deserialize event from notification stream");
                            continue;
                        };

                        if let Err(err) = output.send(event).await {
                            error!("Error sending event: {}", err);
                        }
                    }
                    State::Finished => {
                        let () = futures::future::pending().await;
                    }
                }
            }
        }
    )
}