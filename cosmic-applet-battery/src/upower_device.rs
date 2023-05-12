//! # DBus interface proxy for: `org.freedesktop.UPower.Device`
//!
//! This code was generated by `zbus-xmlgen` `2.0.1` from DBus introspection data.
//! Source: `Interface '/org/freedesktop/UPower/devices/DisplayDevice' from service 'org.freedesktop.UPower' on system bus`.

use cosmic::iced::{self, subscription};

use futures::StreamExt;
use std::{fmt::Debug, hash::Hash};
use zbus::dbus_proxy;

use crate::upower::UPowerProxy;
#[dbus_proxy(
    default_service = "org.freedesktop.UPower",
    interface = "org.freedesktop.UPower.Device"
)]
trait Device {
    /// GetHistory method
    fn get_history(
        &self,
        type_: &str,
        timespan: u32,
        resolution: u32,
    ) -> zbus::Result<Vec<(u32, f64, u32)>>;

    /// GetStatistics method
    fn get_statistics(&self, type_: &str) -> zbus::Result<Vec<(f64, f64)>>;

    /// Refresh method
    fn refresh(&self) -> zbus::Result<()>;

    /// BatteryLevel property
    #[dbus_proxy(property)]
    fn battery_level(&self) -> zbus::Result<u32>;

    /// Capacity property
    #[dbus_proxy(property)]
    fn capacity(&self) -> zbus::Result<f64>;

    /// ChargeCycles property
    #[dbus_proxy(property)]
    fn charge_cycles(&self) -> zbus::Result<i32>;

    /// Energy property
    #[dbus_proxy(property)]
    fn energy(&self) -> zbus::Result<f64>;

    /// EnergyEmpty property
    #[dbus_proxy(property)]
    fn energy_empty(&self) -> zbus::Result<f64>;

    /// EnergyFull property
    #[dbus_proxy(property)]
    fn energy_full(&self) -> zbus::Result<f64>;

    /// EnergyFullDesign property
    #[dbus_proxy(property)]
    fn energy_full_design(&self) -> zbus::Result<f64>;

    /// EnergyRate property
    #[dbus_proxy(property)]
    fn energy_rate(&self) -> zbus::Result<f64>;

    /// HasHistory property
    #[dbus_proxy(property)]
    fn has_history(&self) -> zbus::Result<bool>;

    /// HasStatistics property
    #[dbus_proxy(property)]
    fn has_statistics(&self) -> zbus::Result<bool>;

    /// IconName property
    #[dbus_proxy(property)]
    fn icon_name(&self) -> zbus::Result<String>;

    /// IsPresent property
    #[dbus_proxy(property)]
    fn is_present(&self) -> zbus::Result<bool>;

    /// IsRechargeable property
    #[dbus_proxy(property)]
    fn is_rechargeable(&self) -> zbus::Result<bool>;

    /// Luminosity property
    #[dbus_proxy(property)]
    fn luminosity(&self) -> zbus::Result<f64>;

    /// Model property
    #[dbus_proxy(property)]
    fn model(&self) -> zbus::Result<String>;

    /// NativePath property
    #[dbus_proxy(property)]
    fn native_path(&self) -> zbus::Result<String>;

    /// Online property
    #[dbus_proxy(property)]
    fn online(&self) -> zbus::Result<bool>;

    /// Percentage property
    #[dbus_proxy(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    /// PowerSupply property
    #[dbus_proxy(property)]
    fn power_supply(&self) -> zbus::Result<bool>;

    /// Serial property
    #[dbus_proxy(property)]
    fn serial(&self) -> zbus::Result<String>;

    /// State property
    #[dbus_proxy(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// Technology property
    #[dbus_proxy(property)]
    fn technology(&self) -> zbus::Result<u32>;

    /// Temperature property
    #[dbus_proxy(property)]
    fn temperature(&self) -> zbus::Result<f64>;

    /// TimeToEmpty property
    #[dbus_proxy(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;

    /// TimeToFull property
    #[dbus_proxy(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;

    /// Type property
    #[dbus_proxy(property)]
    fn type_(&self) -> zbus::Result<u32>;

    /// UpdateTime property
    #[dbus_proxy(property)]
    fn update_time(&self) -> zbus::Result<u64>;

    /// Vendor property
    #[dbus_proxy(property)]
    fn vendor(&self) -> zbus::Result<String>;

    /// Voltage property
    #[dbus_proxy(property)]
    fn voltage(&self) -> zbus::Result<f64>;

    /// WarningLevel property
    #[dbus_proxy(property)]
    fn warning_level(&self) -> zbus::Result<u32>;
}

pub fn device_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced::Subscription<Option<(I, DeviceDbusEvent)>> {
    subscription::unfold(id, State::Ready, move |state| start_listening(id, state))
}

#[derive(Debug)]
pub enum State {
    Ready,
    Waiting(DeviceProxy<'static>),
    Finished,
}

async fn display_device() -> zbus::Result<DeviceProxy<'static>> {
    let connection = zbus::Connection::system().await?;
    let upower = UPowerProxy::new(&connection).await?;
    let device_path = upower.get_display_device().await?;
    DeviceProxy::builder(&connection)
        .path(device_path)?
        .cache_properties(zbus::CacheProperties::Yes)
        .build()
        .await
}

async fn start_listening<I: Copy>(id: I, state: State) -> (Option<(I, DeviceDbusEvent)>, State) {
    match state {
        State::Ready => {
            if let Ok(device) = display_device().await {
                return (
                    Some((
                        id,
                        DeviceDbusEvent::Update {
                            icon_name: device
                                .cached_icon_name()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                            percent: device
                                .cached_percentage()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                            time_to_empty: device
                                .cached_time_to_empty()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                        },
                    )),
                    State::Waiting(device),
                );
            }
            (None, State::Finished)
        }
        State::Waiting(device) => {
            let mut stream = futures::stream_select!(
                device.receive_icon_name_changed().await.map(|_| ()),
                device.receive_percentage_changed().await.map(|_| ()),
                device.receive_time_to_empty_changed().await.map(|_| ()),
            );
            match stream.next().await {
                Some(_) => (
                    Some((
                        id,
                        DeviceDbusEvent::Update {
                            icon_name: device
                                .cached_icon_name()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                            percent: device
                                .cached_percentage()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                            time_to_empty: device
                                .cached_time_to_empty()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                        },
                    )),
                    State::Waiting(device),
                ),
                None => (None, State::Finished),
            }
        }
        State::Finished => iced::futures::future::pending().await,
    }
}

#[derive(Debug, Clone)]
pub enum DeviceDbusEvent {
    Update {
        icon_name: String,
        percent: f64,
        time_to_empty: i64,
    },
}
