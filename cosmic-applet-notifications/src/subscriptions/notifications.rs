use cosmic::{
    iced::{
        futures::{self, SinkExt},
        subscription,
    },
    iced_futures::Subscription,
};
use cosmic_notifications_util::AppletEvent;
use sendfd::RecvWithFd;
use std::os::unix::io::{FromRawFd, RawFd};
use tokio::{
    io::{self, AsyncBufReadExt, BufReader},
    net::UnixStream,
};
use tracing::{error, info, warn};

#[derive(Debug)]
pub enum State {
    Ready,
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
            let mut state = State::Ready;

            loop {
                match &mut state {
                    State::Ready => {
                        info!("Reading COSMIC_NOTIFICATIONS env var");
                        let Ok(Some(raw_fd)) = std::env::var("COSMIC_NOTIFICATIONS")
                            .map(|fd| fd.parse::<RawFd>().ok()) else 
                        {
                            error!("Failed to parse COSMIC_NOTIFICATIONS env var");
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
                        info!("Waiting for panel to send us a stream");
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
                                if data_cnt == 0 && fd_cnt == 0 {
                                    warn!("Received EOF from panel");
                                    state = State::Finished;

                                    continue;
                                }
                                if data_cnt != 4 || fd_cnt != 1 {
                                    error!(
                                        "Invalid data received from panel {} {}",
                                        data_cnt, fd_cnt
                                    );
                                    state = State::Finished;

                                    continue;
                                }
                                let notif_stream = unsafe {
                                    std::os::unix::net::UnixStream::from_raw_fd(fd_buf[0])
                                };
                                let Ok(notif_stream) = UnixStream::from_std(notif_stream) else {
                                    error!("Failed to convert raw fd to unix stream");
                                    state = State::Finished;
                                    continue;
                                };

                                state = State::WaitingForNotificationEvent(notif_stream);
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                continue;
                            }
                            Err(err) => {
                                error!("Failed to receive fd from panel: {}", err);
                                state = State::Finished;

                                continue;
                            }
                        }
                    }
                    State::WaitingForNotificationEvent(stream) => {
                        info!("Waiting for notification event");
                        let reader = BufReader::new(stream);
                        // todo read messages

                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            if line.is_empty() {
                                warn!("Received empty line from notification stream. The notification daemon probably crashed, so we will exit.");
                                std::process::exit(1);
                            }
                            if let Ok(event) = ron::de::from_str::<AppletEvent>(line.as_str()) {
                                if let Err(_err) = output.send(event).await {
                                    error!("Error sending event");
                                }
                            } else {
                                error!("Failed to deserialize event from notification stream");
                            }
                        }
                        warn!("Notification stream closed. The notification daemon probably crashed, so we will exit.");
                        std::process::exit(1);
                    }
                    State::Finished => {
                        let () = futures::future::pending().await;
                    }
                }
            }
        },
    )
}
