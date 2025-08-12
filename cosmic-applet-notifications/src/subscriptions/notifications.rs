// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    iced::{
        futures::{self, FutureExt},
        stream,
    },
    iced_futures::Subscription,
};
use cosmic_notifications_util::Notification;
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::HashMap,
    os::unix::io::{FromRawFd, RawFd},
    pin::pin,
};
use tokio::sync::mpsc;
use tracing::{error, trace};
use zbus::{connection::Builder, proxy};

#[derive(Debug)]
pub enum State {
    WaitingForNotificationEvent,
    Finished,
}

#[derive(Debug, Clone)]
pub enum Input {
    Activated(u32, String),
}

#[derive(Debug, Clone)]
pub enum Output {
    Ready(mpsc::Sender<Input>),
    Notification(Notification),
}

pub fn notifications(proxy: NotificationsAppletProxy<'static>) -> Subscription<Output> {
    struct SomeWorker;

    Subscription::run_with_id(
        std::any::TypeId::of::<SomeWorker>(),
        stream::channel(50, |mut output| async move {
            let mut state = State::WaitingForNotificationEvent;
            let (sender, mut receiver) = mpsc::channel(10);
            _ = output.send(Output::Ready(sender)).await;

            let mut signal;
            let mut fail_count: u8 = 0;
            loop {
                match proxy.receive_notify().await {
                    Ok(s) => {
                        signal = s;
                        break;
                    }
                    Err(err) => {
                        error!(
                            "failed to get a stream of signals for notifications. {}",
                            err
                        );
                        fail_count = fail_count.saturating_add(1);
                        if fail_count > 5 {
                            error!("Failed to receive notification events");
                            // exit because the applet needs the notifications daemon in order to work properly
                            std::process::exit(0);
                        } else {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        };
                        continue;
                    }
                }
            }
            loop {
                match &mut state {
                    State::WaitingForNotificationEvent => {
                        trace!("Waiting for notification events...");
                        let mut next_signal = signal.next();
                        let mut next_input = pin!(receiver.recv().fuse());
                        cosmic::iced::futures::select! {
                            v = next_signal => {
                                if let Some(msg) = v {
                                    let Some(args) = msg.args().into_iter().next() else {
                                        break;
                                    };
                                    let notification = Notification::new(
                                        args.app_name,
                                        args.id,
                                        args.app_icon,
                                        args.summary,
                                        args.body,
                                        args.actions,
                                        args.hints,
                                        args.expire_timeout,
                                    );
                                    _ = output.send(Output::Notification(notification)).await;
                                } else {
                                    tracing::error!("Signal stream closed, ending notifications subscription");
                                    state = State::Finished;
                                }
                            }
                            v = next_input => {
                                if let Some(Input::Activated(id, action)) = v {
                                    if let Err(err) = proxy.invoke_action(id, action.clone()).await {
                                        tracing::error!("Failed to invoke action {id} {action}");
                                    } else {
                                        tracing::error!("Invoked {action} for {id}")
                                    }
                                } else {
                                    tracing::error!("Channel closed, ending notifications subscription");
                                    state = State::Finished;
                                }
                            }
                        }
                    }
                    State::Finished => {
                        let () = futures::future::pending().await;
                    }
                }
            }
        }),
    )
}

#[proxy(
    default_service = "com.system76.NotificationsApplet",
    interface = "com.system76.NotificationsApplet",
    default_path = "/com/system76/NotificationsApplet"
)]
pub trait NotificationsApplet {
    #[zbus(signal)]
    fn notify(
        &self,
        app_name: &str,
        id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<&str>,
        hints: HashMap<&str, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<()>;

    fn invoke_action(&self, id: u32, action: String) -> zbus::Result<()>;
}

pub async fn get_proxy() -> anyhow::Result<NotificationsAppletProxy<'static>> {
    let raw_fd = std::env::var("COSMIC_NOTIFICATIONS")?;
    let raw_fd = raw_fd.parse::<RawFd>()?;
    tracing::info!("Connecting to notifications daemon on fd {}", raw_fd);

    let stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(raw_fd) };
    stream.set_nonblocking(true)?;
    let stream = tokio::net::UnixStream::from_std(stream)?;
    let conn = Builder::socket(stream).p2p().build().await?;
    trace!("Applet connection created");
    let proxy = NotificationsAppletProxy::new(&conn).await?;

    Ok(proxy)
}
