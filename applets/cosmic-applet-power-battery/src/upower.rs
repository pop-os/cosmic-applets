//! # DBus interface proxy for: `org.freedesktop.UPower`
//!
//! This code was generated by `zbus-xmlgen` `2.0.1` from DBus introspection data.
//! Source: `Interface '/org/freedesktop/UPower' from service 'org.freedesktop.UPower' on system bus`.

use zbus::dbus_proxy;

#[dbus_proxy(interface = "org.freedesktop.UPower")]
trait UPower {
    /// EnumerateDevices method
    fn enumerate_devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    /// GetCriticalAction method
    fn get_critical_action(&self) -> zbus::Result<String>;

    /// GetDisplayDevice method
    fn get_display_device(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// DeviceAdded signal
    #[dbus_proxy(signal)]
    fn device_added(&self, device: zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// DeviceRemoved signal
    #[dbus_proxy(signal)]
    fn device_removed(&self, device: zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// DaemonVersion property
    #[dbus_proxy(property)]
    fn daemon_version(&self) -> zbus::Result<String>;

    /// LidIsClosed property
    #[dbus_proxy(property)]
    fn lid_is_closed(&self) -> zbus::Result<bool>;

    /// LidIsPresent property
    #[dbus_proxy(property)]
    fn lid_is_present(&self) -> zbus::Result<bool>;

    /// OnBattery property
    #[dbus_proxy(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}
