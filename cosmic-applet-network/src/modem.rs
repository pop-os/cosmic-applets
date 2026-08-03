// SPDX-License-Identifier: GPL-3.0-or-later

//! ModemManager integration for mobile broadband state shown by the network applet.

use std::time::Duration;

use futures::StreamExt;
use zbus::{Connection, fdo::ObjectManagerProxy, proxy};

const MODEM_MANAGER_SERVICE: &str = "org.freedesktop.ModemManager1";
const MODEM_MANAGER_PATH: &str = "/org/freedesktop/ModemManager1";
const MODEM_INTERFACE: &str = "org.freedesktop.ModemManager1.Modem";

// `MMModemAccessTechnology` bit flags from ModemManager's public D-Bus API.
const ACCESS_TECHNOLOGY_GPRS: u32 = 1 << 0;
const ACCESS_TECHNOLOGY_EDGE: u32 = 1 << 1;
const ACCESS_TECHNOLOGY_UMTS: u32 = 1 << 2;
const ACCESS_TECHNOLOGY_HSDPA: u32 = 1 << 3;
const ACCESS_TECHNOLOGY_HSUPA: u32 = 1 << 4;
const ACCESS_TECHNOLOGY_HSPA: u32 = 1 << 5;
const ACCESS_TECHNOLOGY_HSPA_PLUS: u32 = 1 << 6;
const ACCESS_TECHNOLOGY_LTE: u32 = 1 << 14;
const ACCESS_TECHNOLOGY_5GNR: u32 = 1 << 15;
const ACCESS_TECHNOLOGY_5GNR_MMWAVE: u32 = 1 << 16;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Technology {
    #[default]
    Unknown,
    TwoG,
    ThreeG,
    FourG,
    FiveG,
}

impl Technology {
    #[must_use]
    pub fn from_access_technologies(access_technologies: u32) -> Self {
        if access_technologies & (ACCESS_TECHNOLOGY_5GNR | ACCESS_TECHNOLOGY_5GNR_MMWAVE) != 0 {
            Self::FiveG
        } else if access_technologies & ACCESS_TECHNOLOGY_LTE != 0 {
            Self::FourG
        } else if access_technologies
            & (ACCESS_TECHNOLOGY_UMTS
                | ACCESS_TECHNOLOGY_HSDPA
                | ACCESS_TECHNOLOGY_HSUPA
                | ACCESS_TECHNOLOGY_HSPA
                | ACCESS_TECHNOLOGY_HSPA_PLUS)
            != 0
        {
            Self::ThreeG
        } else if access_technologies & (ACCESS_TECHNOLOGY_GPRS | ACCESS_TECHNOLOGY_EDGE) != 0 {
            Self::TwoG
        } else {
            Self::Unknown
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Mobile",
            Self::TwoG => "2G",
            Self::ThreeG => "3G",
            Self::FourG => "4G",
            Self::FiveG => "5G",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Status {
    /// Network interface exposed by ModemManager, e.g. `wwan0mbim0`.
    pub interface: String,
    pub technology: Technology,
    /// Percentage reported by ModemManager. `None` means no recent measurement.
    pub signal_quality: Option<u8>,
}

impl Status {
    #[must_use]
    pub fn signal_icon(&self) -> &'static str {
        match self.signal_quality.unwrap_or_default() {
            0 => "network-cellular-signal-none-symbolic",
            1..=24 => "network-cellular-signal-weak-symbolic",
            25..=49 => "network-cellular-signal-ok-symbolic",
            50..=74 => "network-cellular-signal-good-symbolic",
            _ => "network-cellular-signal-excellent-symbolic",
        }
    }
}

#[proxy(
    interface = "org.freedesktop.ModemManager1.Modem",
    default_service = "org.freedesktop.ModemManager1"
)]
trait Modem {
    #[zbus(property, name = "AccessTechnologies")]
    fn access_technologies(&self) -> zbus::Result<u32>;

    #[zbus(property, name = "PrimaryPort")]
    fn primary_port(&self) -> zbus::Result<String>;

    #[zbus(property, name = "SignalQuality")]
    fn signal_quality(&self) -> zbus::Result<(u32, bool)>;

}

/// Subscribes to ModemManager lifecycle changes and refreshes all modem
/// properties at a short fixed interval. Polling avoids subscribing only to
/// the first modem when more than one is present, while still keeping signal
/// and radio-technology updates close to real time.
pub fn events_task() -> cosmic::Task<crate::app::Message> {
    cosmic::Task::stream(async_fn_stream::fn_stream(|emitter| async move {
        let connection = match Connection::system().await {
            Ok(connection) => connection,
            Err(error) => {
                let _ = emitter
                    .emit(Err(format!("connect to ModemManager: {error}")))
                    .await;
                return;
            }
        };

        let manager = match ObjectManagerProxy::builder(&connection)
            .destination(MODEM_MANAGER_SERVICE)
            .and_then(|builder| builder.path(MODEM_MANAGER_PATH))
        {
            Ok(builder) => match builder.build().await {
                Ok(manager) => manager,
                Err(error) => {
                    let _ = emitter
                        .emit(Err(format!("connect to ModemManager: {error}")))
                        .await;
                    return;
                }
            },
            Err(error) => {
                let _ = emitter
                    .emit(Err(format!("build ModemManager proxy: {error}")))
                    .await;
                return;
            }
        };

        let mut interfaces_added = match manager.receive_interfaces_added().await {
            Ok(stream) => stream,
            Err(error) => {
                let _ = emitter
                    .emit(Err(format!("watch ModemManager devices: {error}")))
                    .await;
                return;
            }
        };
        let mut interfaces_removed = match manager.receive_interfaces_removed().await {
            Ok(stream) => stream,
            Err(error) => {
                let _ = emitter
                    .emit(Err(format!("watch ModemManager devices: {error}")))
                    .await;
                return;
            }
        };

        loop {
            let paths = match modem_paths(&manager).await {
                Ok(paths) => paths,
                Err(error) => {
                    let _ = emitter.emit(Err(error)).await;
                    return;
                }
            };

            match read_statuses(&connection, &paths).await {
                Ok(statuses) => {
                    emitter.emit(Ok(statuses)).await;
                }
                Err(error) => {
                    emitter.emit(Err(error)).await;
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {},
                _ = interfaces_added.next() => {},
                _ = interfaces_removed.next() => {},
            }
        }
    }))
    .map(|status| match status {
        Ok(status) => crate::app::Message::ModemStatus(status),
        Err(error) => crate::app::Message::Error(error),
    })
}

async fn modem_paths(manager: &ObjectManagerProxy<'_>) -> Result<Vec<String>, String> {
    let objects = manager
        .get_managed_objects()
        .await
        .map_err(|error| format!("enumerate ModemManager devices: {error}"))?;

    let mut paths = objects
        .into_iter()
        .filter(|(_, interfaces)| interfaces.contains_key(MODEM_INTERFACE))
        .map(|(path, _)| path.to_string())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

async fn read_statuses(connection: &Connection, paths: &[String]) -> Result<Vec<Status>, String> {
    let mut statuses = Vec::with_capacity(paths.len());
    let mut last_error = None;
    for path in paths {
        match read_status(connection, path).await {
            Ok(status) => statuses.push(status),
            Err(error) => last_error = Some(error),
        }
    }

    if statuses.is_empty() {
        return last_error.map_or_else(|| Ok(statuses), Err);
    }

    Ok(statuses)
}

async fn read_status(connection: &Connection, path: &str) -> Result<Status, String> {
    let proxy = ModemProxy::builder(connection)
        .path(path)
        .map_err(|error| format!("build modem proxy: {error}"))?
        .build()
        .await
        .map_err(|error| format!("read modem properties: {error}"))?;
    let (interface, access_technologies, signal_quality) = futures::join!(
        proxy.primary_port(),
        proxy.access_technologies(),
        proxy.signal_quality(),
    );
    let (signal_quality, recent) = signal_quality
        .map_err(|error| format!("read modem signal quality: {error}"))?;

    Ok(Status {
        interface: interface.map_err(|error| format!("read modem interface: {error}"))?,
        technology: Technology::from_access_technologies(
            access_technologies.map_err(|error| format!("read modem access technology: {error}"))?,
        ),
        signal_quality: recent.then_some(signal_quality.min(100) as u8),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favors_5g_when_lte_and_5g_are_reported_together() {
        assert_eq!(
            Technology::from_access_technologies(ACCESS_TECHNOLOGY_LTE | ACCESS_TECHNOLOGY_5GNR),
            Technology::FiveG
        );
    }

    #[test]
    fn classifies_each_supported_radio_generation() {
        assert_eq!(
            Technology::from_access_technologies(ACCESS_TECHNOLOGY_GPRS),
            Technology::TwoG
        );
        assert_eq!(
            Technology::from_access_technologies(ACCESS_TECHNOLOGY_HSPA_PLUS),
            Technology::ThreeG
        );
        assert_eq!(
            Technology::from_access_technologies(ACCESS_TECHNOLOGY_LTE),
            Technology::FourG
        );
        assert_eq!(
            Technology::from_access_technologies(ACCESS_TECHNOLOGY_5GNR_MMWAVE),
            Technology::FiveG
        );
    }

    #[test]
    fn maps_signal_quality_to_standard_icon_levels() {
        let status = |signal_quality| Status {
            signal_quality,
            ..Status::default()
        };
        assert_eq!(status(Some(24)).signal_icon(), "network-cellular-signal-weak-symbolic");
        assert_eq!(status(Some(25)).signal_icon(), "network-cellular-signal-ok-symbolic");
        assert_eq!(status(Some(75)).signal_icon(), "network-cellular-signal-excellent-symbolic");
        assert_eq!(status(None).signal_icon(), "network-cellular-signal-none-symbolic");
    }
}
