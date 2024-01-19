use cosmic::{
    iced::{futures, subscription},
    iced_futures::Subscription,
};
use cosmic_notifications_util::Notification;
use std::{
    collections::HashMap,
    os::unix::io::{FromRawFd, RawFd},
};

use tracing::{error, trace};
use zbus::{
    dbus_proxy,
    export::futures_util::{SinkExt, StreamExt},
    ConnectionBuilder,
};

#[derive(Debug)]
pub enum State {
    WaitingForNotificationEvent(u8),
    Finished,
}

pub fn notifications(proxy: NotificationsAppletProxy<'static>) -> Subscription<Notification> {
    struct SomeWorker;

    subscription::channel(
        std::any::TypeId::of::<SomeWorker>(),
        50,
        |mut output| async move {
            let mut state = State::WaitingForNotificationEvent(0);

            loop {
                match &mut state {
                    State::WaitingForNotificationEvent(mut fail_count) => {
                        trace!("Waiting for notification events...");
                        let mut signal = match proxy.receive_notify().await {
                            Ok(s) => s,
                            Err(err) => {
                                error!(
                                    "failed to get a stream of signals for notifications. {}",
                                    err
                                );
                                fail_count = fail_count.saturating_add(1);
                                if fail_count > 5 {
                                    error!("Failed to receive notification events");
                                    state = State::Finished;
                                } else {
                                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                };
                                continue;
                            }
                        };
                        while let Some(msg) = signal.next().await {
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
                            _ = output.send(notification).await;
                        }
                    }
                    State::Finished => {
                        let () = futures::future::pending().await;
                    }
                }
            }
        },
    )
}

#[dbus_proxy(
    default_service = "com.system76.NotificationsApplet",
    interface = "com.system76.NotificationsApplet",
    default_path = "/com/system76/NotificationsApplet"
)]
trait NotificationsApplet {
    #[dbus_proxy(signal)]
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
}

pub async fn get_proxy() -> anyhow::Result<NotificationsAppletProxy<'static>> {
    let raw_fd = std::env::var("COSMIC_NOTIFICATIONS")?;
    let raw_fd = raw_fd.parse::<RawFd>()?;
    tracing::info!("Connecting to notifications daemon on fd {}", raw_fd);

    let stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(raw_fd) };
    stream.set_nonblocking(true)?;
    let stream = tokio::net::UnixStream::from_std(stream)?;
    let conn = ConnectionBuilder::socket(stream).p2p().build().await?;
    trace!("Applet connection created");
    let proxy = NotificationsAppletProxy::new(&conn).await?;

    Ok(proxy)
}
