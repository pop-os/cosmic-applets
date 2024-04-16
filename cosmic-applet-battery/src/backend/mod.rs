use cosmic::iced::{self, futures::SinkExt, subscription};
use std::fmt::Debug;
use std::hash::Hash;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use zbus::Connection;
use zbus::Result;

use self::power_daemon::PowerDaemonProxy;
use self::power_profiles::PowerProfilesProxy;

mod power_daemon;
mod power_profiles;

#[derive(PartialEq, Eq, Copy, Clone, Debug, Default)]
pub enum Power {
    Battery,
    #[default]
    Balanced,
    Performance,
}

#[derive(Debug)]
pub enum Backend<'a> {
    S76PowerDaemon(PowerDaemonProxy<'a>),
    PowerProfilesDaemon(PowerProfilesProxy<'a>),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum BackendType {
    #[default]
    S76PowerDaemon,
    PowerProfilesDaemon,
}

impl BackendType {
    fn next(self) -> Self {
        match self {
            Self::S76PowerDaemon => Self::PowerProfilesDaemon,
            Self::PowerProfilesDaemon => Self::S76PowerDaemon,
        }
    }
}

pub async fn get_power_profile(daemon: Backend<'_>) -> Result<Power> {
    match daemon {
        Backend::S76PowerDaemon(p) => {
            let power = p.get_profile().await?;
            match power.as_str() {
                "Battery" => Ok(Power::Battery),
                "Balanced" => Ok(Power::Balanced),
                "Performance" => Ok(Power::Performance),
                _ => panic!("Unknown power profile: {}", power),
            }
        }
        Backend::PowerProfilesDaemon(ppd) => {
            let power = ppd.active_profile().await?;
            match power.as_str() {
                "power-saver" => Ok(Power::Battery),
                "balanced" => Ok(Power::Balanced),
                "performance" => Ok(Power::Performance),
                _ => panic!("Unknown power profile: {}", power),
            }
        }
    }
}

pub async fn set_power_profile(daemon: Backend<'_>, power: Power) -> Result<()> {
    match daemon {
        Backend::S76PowerDaemon(p) => match power {
            Power::Battery => p.battery().await,
            Power::Balanced => p.balanced().await,
            Power::Performance => p.performance().await,
        },
        Backend::PowerProfilesDaemon(ppd) => match power {
            Power::Battery => ppd.set_active_profile("power-saver").await,
            Power::Balanced => ppd.set_active_profile("balanced").await,
            Power::Performance => ppd.set_active_profile("performance").await,
        },
    }
}

pub fn power_profile_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced::Subscription<PowerProfileUpdate> {
    subscription::channel(id, 50, move |mut output| async move {
        let mut state = State::Ready;

        loop {
            state = start_listening(state, &mut output).await;
        }
    })
}

#[derive(Debug)]
pub enum State {
    Ready,
    Connecting(BackendType),
    Waiting(
        Connection,
        UnboundedReceiver<PowerProfileRequest>,
        BackendType,
    ),
    Finished,
}

async fn start_listening(
    state: State,
    output: &mut futures::channel::mpsc::Sender<PowerProfileUpdate>,
) -> State {
    match state {
        State::Ready => {
            // Default to s76 powerdaemon
            State::Connecting(BackendType::default())
        }
        State::Connecting(backend_type) => {
            let conn = match Connection::system().await.map_err(|e| e.to_string()) {
                Ok(conn) => conn,
                Err(e) => {
                    _ = output.send(PowerProfileUpdate::Error(e)).await;
                    return State::Finished;
                }
            };
            let backend = match backend_type {
                BackendType::S76PowerDaemon => {
                    match PowerDaemonProxy::new(&conn)
                        .await
                        .map_err(|e| e.to_string())
                    {
                        Ok(p) => Backend::S76PowerDaemon(p),
                        Err(e) => {
                            _ = output.send(PowerProfileUpdate::Error(e)).await;
                            return State::Connecting(backend_type.next());
                        }
                    }
                }
                BackendType::PowerProfilesDaemon => {
                    match PowerProfilesProxy::new(&conn)
                        .await
                        .map_err(|e| e.to_string())
                    {
                        Ok(p) => Backend::PowerProfilesDaemon(p),
                        Err(e) => {
                            _ = output.send(PowerProfileUpdate::Error(e)).await;
                            return State::Connecting(backend_type.next());
                        }
                    }
                }
            };
            // Successful connection
            let profile = match get_power_profile(backend).await.map_err(|e| e.to_string()) {
                Ok(p) => p,
                Err(e) => {
                    _ = output.send(PowerProfileUpdate::Error(e)).await;
                    return State::Connecting(backend_type.next());
                }
            };
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            _ = output.send(PowerProfileUpdate::Init(profile, tx)).await;
            State::Waiting(conn, rx, backend_type)
        }
        State::Waiting(conn, mut rx, backend_type) => {
            let backend = match backend_type {
                BackendType::S76PowerDaemon => {
                    match PowerDaemonProxy::new(&conn)
                        .await
                        .map_err(|e| e.to_string())
                    {
                        Ok(p) => Backend::S76PowerDaemon(p),
                        Err(e) => {
                            _ = output.send(PowerProfileUpdate::Error(e)).await;
                            return State::Connecting(backend_type);
                        }
                    }
                }
                BackendType::PowerProfilesDaemon => {
                    match PowerProfilesProxy::new(&conn)
                        .await
                        .map_err(|e| e.to_string())
                    {
                        Ok(p) => Backend::PowerProfilesDaemon(p),
                        Err(e) => {
                            _ = output.send(PowerProfileUpdate::Error(e)).await;
                            return State::Connecting(backend_type);
                        }
                    }
                }
            };

            match rx.recv().await {
                Some(PowerProfileRequest::Get) => {
                    if let Ok(profile) = get_power_profile(backend).await {
                        _ = output.send(PowerProfileUpdate::Update { profile }).await;
                    }
                    State::Waiting(conn, rx, backend_type)
                }
                Some(PowerProfileRequest::Set(profile)) => {
                    let _ = set_power_profile(backend, profile).await;
                    _ = output.send(PowerProfileUpdate::Update { profile }).await;
                    State::Waiting(conn, rx, backend_type)
                }
                None => State::Finished,
            }
        }
        State::Finished => iced::futures::future::pending().await,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PowerProfileRequest {
    Get,
    Set(Power),
}

#[derive(Debug, Clone)]
pub enum PowerProfileUpdate {
    Init(Power, UnboundedSender<PowerProfileRequest>),
    Update { profile: Power },
    Error(String),
}
