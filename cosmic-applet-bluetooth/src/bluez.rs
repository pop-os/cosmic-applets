// SPDX-License-Identifier: GPL-3.0-only

//! BlueZ over zbus, exposing the subset of the bluer API used by this applet.

use std::{
    collections::HashMap,
    fmt,
    pin::Pin,
    str::FromStr,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use bluez_zbus::{
    adapter1::Adapter1Proxy, agent_manager1::AgentManager1Proxy, battery1::Battery1Proxy,
    device1::Device1Proxy,
};
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use zbus::{
    MatchRule, MessageStream, fdo,
    message::Type as MessageType,
    proxy::CacheProperties,
    zvariant::{OwnedObjectPath, Value},
};

pub type Error = zbus::Error;
pub type Result<T> = std::result::Result<T, Error>;

const SERVICE: &str = "org.bluez";
const PREFIX: &str = "/org/bluez/";
const DEFAULT_ADAPTER: &str = "hci0";
const DEVICE_INTERFACE: &str = "org.bluez.Device1";
const ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
// Same method call timeout as bluer.
const TIMEOUT: Duration = Duration::from_secs(120);
// Device properties that bluer reports as change events.
const DEVICE_PROPERTIES: &[&str] = &[
    "Name",
    "Address",
    "AddressType",
    "Icon",
    "Class",
    "Appearance",
    "UUIDs",
    "Paired",
    "Connected",
    "Trusted",
    "Blocked",
    "WakeAllowed",
    "Alias",
    "LegacyPairing",
    "Modalias",
    "RSSI",
    "TxPower",
    "ManufacturerData",
    "ServiceData",
    "ServicesResolved",
    "AdvertisingFlags",
    "AdvertisingData",
    "Percentage",
];

/// Bluetooth device address.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address(pub [u8; 6]);

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = self.0;
        write!(f, "{a:02X}:{b:02X}:{c:02X}:{d:02X}:{e:02X}:{g:02X}")
    }
}

impl FromStr for Address {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        let mut bytes = [0u8; 6];
        let mut parts = s.split(':');
        for byte in &mut bytes {
            let part = parts.next().ok_or(())?;
            if part.len() != 2 {
                return Err(());
            }
            *byte = u8::from_str_radix(part, 16).map_err(|_| ())?;
        }
        if parts.next().is_some() {
            return Err(());
        }
        Ok(Self(bytes))
    }
}

/// `/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF` gives `("hci0", AA:BB:CC:DD:EE:FF)`.
fn parse_device_path(path: &str) -> Option<(&str, Address)> {
    let rest = path.strip_prefix(PREFIX)?;
    let (adapter, device) = rest.split_once('/')?;
    let address = device.strip_prefix("dev_")?;
    if address.contains('/') {
        return None;
    }
    Some((adapter, address.replace('_', ":").parse().ok()?))
}

fn device_path(adapter: &str, address: Address) -> Result<OwnedObjectPath> {
    let address = address.to_string().replace(':', "_");
    OwnedObjectPath::try_from(format!("{PREFIX}{adapter}/dev_{address}")).map_err(Error::from)
}

/// Missing optional property or interface, like bluer.
fn optional<T>(result: Result<T>) -> Result<Option<T>> {
    const MISSING: &[&str] = &[
        "org.freedesktop.DBus.Error.InvalidArgs",
        "org.freedesktop.DBus.Error.UnknownProperty",
        "org.freedesktop.DBus.Error.UnknownInterface",
    ];
    match result {
        Ok(value) => Ok(Some(value)),
        Err(Error::FDO(error))
            if matches!(
                *error,
                fdo::Error::InvalidArgs(_)
                    | fdo::Error::UnknownProperty(_)
                    | fdo::Error::UnknownInterface(_)
            ) =>
        {
            Ok(None)
        }
        Err(Error::MethodError(name, _, _)) if MISSING.contains(&name.as_str()) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn managed_objects(connection: &zbus::Connection) -> Result<fdo::ManagedObjects> {
    let manager = fdo::ObjectManagerProxy::new(connection, SERVICE, "/").await?;
    Ok(manager.get_managed_objects().await?)
}

pub struct Session {
    connection: zbus::Connection,
}

impl Session {
    pub async fn new() -> Result<Self> {
        let connection = zbus::connection::Builder::system()?
            .method_timeout(TIMEOUT)
            .build()
            .await?;
        Ok(Self { connection })
    }

    /// `hci0` if present, otherwise the first adapter by name.
    pub async fn default_adapter(&self) -> Result<Adapter> {
        let mut names: Vec<String> = managed_objects(&self.connection)
            .await?
            .into_iter()
            .filter(|(_, interfaces)| interfaces.contains_key(ADAPTER_INTERFACE))
            .filter_map(|(path, _)| {
                let name = path.as_str().strip_prefix(PREFIX)?;
                (!name.is_empty() && !name.contains('/')).then(|| name.to_owned())
            })
            .collect();
        let name = if names.iter().any(|name| name == DEFAULT_ADAPTER) {
            DEFAULT_ADAPTER.to_owned()
        } else {
            names.sort();
            names
                .into_iter()
                .next()
                .ok_or_else(|| Error::Failure("no Bluetooth adapter".to_owned()))?
        };
        Adapter::new(self.connection.clone(), name).await
    }

    pub async fn register_agent(&self, agent: agent::Agent) -> Result<agent::AgentHandle> {
        agent::register(self.connection.clone(), agent).await
    }
}

// Never emitted by `discover_devices_with_changes`, same as bluer.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum AdapterProperty {
    Powered(bool),
    Discovering(bool),
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum AdapterEvent {
    DeviceAdded(Address),
    DeviceRemoved(Address),
    PropertyChanged(AdapterProperty),
}

#[derive(Clone)]
pub struct Adapter {
    connection: zbus::Connection,
    name: Arc<str>,
    path: OwnedObjectPath,
    proxy: Adapter1Proxy<'static>,
}

impl Adapter {
    async fn new(connection: zbus::Connection, name: String) -> Result<Self> {
        let path = OwnedObjectPath::try_from(format!("{PREFIX}{name}"))?;
        let proxy = Adapter1Proxy::builder(&connection)
            .path(path.clone())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        Ok(Self {
            connection,
            name: name.into(),
            path,
            proxy,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub async fn is_powered(&self) -> Result<bool> {
        self.proxy.powered().await
    }

    pub async fn set_powered(&self, powered: bool) -> Result<()> {
        self.proxy.set_powered(powered).await
    }

    pub async fn device_addresses(&self) -> Result<Vec<Address>> {
        Ok(managed_objects(&self.connection)
            .await?
            .into_iter()
            .filter(|(_, interfaces)| interfaces.contains_key(DEVICE_INTERFACE))
            .filter_map(|(path, _)| match parse_device_path(path.as_str()) {
                Some((adapter, address)) if adapter == &*self.name => Some(address),
                _ => None,
            })
            .collect())
    }

    pub fn device(&self, address: Address) -> Result<Device> {
        Ok(Device {
            connection: self.connection.clone(),
            path: device_path(&self.name, address)?,
            address,
        })
    }

    /// Starts discovery and reports every known or new device, then a `DeviceAdded`
    /// for each device property change. Discovery stops when the stream is dropped.
    pub async fn discover_devices_with_changes(&self) -> Result<EventStream> {
        let filter = HashMap::from([
            ("UUIDs", Value::from(Vec::<String>::new())),
            ("Transport", Value::from("auto")),
        ]);
        self.proxy
            .set_discovery_filter(filter.iter().map(|(k, v)| (*k, v)).collect())
            .await?;
        self.proxy.start_discovery().await?;
        let guard = DiscoveryGuard(Some(self.proxy.clone()));

        // Object manager signals from any path, like bluer.
        let signals = |interface, member| {
            MatchRule::builder()
                .msg_type(MessageType::Signal)
                .sender(SERVICE)?
                .interface(interface)?
                .member(member)
                .map(|rule| rule.build())
        };
        let manager = "org.freedesktop.DBus.ObjectManager";
        let mut added = MessageStream::for_match_rule(
            signals(manager, "InterfacesAdded")?,
            &self.connection,
            None,
        )
        .await?;
        let mut removed = MessageStream::for_match_rule(
            signals(manager, "InterfacesRemoved")?,
            &self.connection,
            None,
        )
        .await?;
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender(SERVICE)?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .path_namespace(self.path.clone())?
            .build();
        let mut changes = MessageStream::for_match_rule(rule, &self.connection, None).await?;
        let known = self.device_addresses().await?;

        let (tx, rx) = mpsc::channel(1);
        let name = self.name.clone();
        let adapter_path = self.path.to_string();

        tokio::spawn(async move {
            let _guard = guard;
            // Number of change subscriptions per device, as bluer counts them.
            let mut watched: HashMap<Address, usize> = HashMap::new();

            for address in known {
                *watched.entry(address).or_default() += 1;
                if tx.send(AdapterEvent::DeviceAdded(address)).await.is_err() {
                    return;
                }
            }

            loop {
                let mut events = Vec::new();
                tokio::select! {
                    message = added.next() => {
                        let Some(Ok(message)) = message else { return };
                        let Some(signal) = fdo::InterfacesAdded::from_message(message) else { continue };
                        let Ok(args) = signal.args() else { continue };
                        if let Some((adapter, address)) = parse_device_path(args.object_path.as_str()) {
                            if adapter == &*name {
                                *watched.entry(address).or_default() += 1;
                                events.push(AdapterEvent::DeviceAdded(address));
                            }
                        }
                    }
                    message = removed.next() => {
                        let Some(Ok(message)) = message else { return };
                        let Some(signal) = fdo::InterfacesRemoved::from_message(message) else { continue };
                        let Ok(args) = signal.args() else { continue };
                        if let Some((adapter, address)) = parse_device_path(args.object_path.as_str()) {
                            if adapter == &*name {
                                watched.remove(&address);
                                events.push(AdapterEvent::DeviceRemoved(address));
                            }
                        }
                    }
                    message = changes.next() => {
                        let Some(Ok(message)) = message else { return };
                        let Some(signal) = fdo::PropertiesChanged::from_message(message) else { continue };
                        let Ok(args) = signal.args() else { continue };
                        let header = signal.message().header();
                        let Some(path) = header.path() else { continue };
                        if path.as_str() == adapter_path {
                            // Discovery ended elsewhere (adapter off, for example).
                            if let Some(Value::Bool(false)) = args.changed_properties.get("Discovering") {
                                return;
                            }
                        } else if let Some((_, address)) = parse_device_path(path.as_str()) {
                            let subscriptions = watched.get(&address).copied().unwrap_or_default();
                            let count = args
                                .changed_properties
                                .keys()
                                .filter(|key| DEVICE_PROPERTIES.contains(key))
                                .count();
                            for _ in 0..subscriptions * count {
                                events.push(AdapterEvent::DeviceAdded(address));
                            }
                        }
                    }
                    () = tx.closed() => return,
                }

                for event in events {
                    if tx.send(event).await.is_err() {
                        return;
                    }
                }
            }
        });

        Ok(EventStream(rx))
    }
}

/// Stops the discovery started by the stream when it goes away.
struct DiscoveryGuard(Option<Adapter1Proxy<'static>>);

impl Drop for DiscoveryGuard {
    fn drop(&mut self) {
        let Some(proxy) = self.0.take() else { return };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                _ = proxy.stop_discovery().await;
            });
        }
    }
}

pub struct EventStream(mpsc::Receiver<AdapterEvent>);

impl Stream for EventStream {
    type Item = AdapterEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

pub struct Device {
    connection: zbus::Connection,
    path: OwnedObjectPath,
    address: Address,
}

impl Device {
    async fn proxy(&self) -> Result<Device1Proxy<'static>> {
        Device1Proxy::builder(&self.connection)
            .path(self.path.clone())?
            .cache_properties(CacheProperties::No)
            .build()
            .await
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub async fn alias(&self) -> Result<String> {
        self.proxy().await?.alias().await
    }

    pub async fn name(&self) -> Result<Option<String>> {
        optional(self.proxy().await?.name().await)
    }

    pub async fn icon(&self) -> Result<Option<String>> {
        optional(self.proxy().await?.icon().await)
    }

    pub async fn is_paired(&self) -> Result<bool> {
        self.proxy().await?.paired().await
    }

    pub async fn is_trusted(&self) -> Result<bool> {
        self.proxy().await?.trusted().await
    }

    pub async fn is_connected(&self) -> Result<bool> {
        self.proxy().await?.connected().await
    }

    pub async fn battery_percentage(&self) -> Result<Option<u8>> {
        let battery = Battery1Proxy::builder(&self.connection)
            .path(self.path.clone())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        optional(battery.percentage().await)
    }

    pub async fn set_trusted(&self, trusted: bool) -> Result<()> {
        self.proxy().await?.set_trusted(trusted).await
    }

    pub async fn pair(&self) -> Result<()> {
        self.proxy().await?.pair().await
    }

    pub async fn connect(&self) -> Result<()> {
        self.proxy().await?.connect().await
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.proxy().await?.disconnect().await
    }
}

pub mod agent {
    //! `org.bluez.Agent1`, with the same callbacks and replies as the bluer agent.

    use super::{Address, AgentManager1Proxy, parse_device_path};
    use std::{future::Future, pin::Pin};
    use tokio::sync::{Mutex, oneshot};
    use zbus::zvariant::OwnedObjectPath;

    const PATH: &str = "/org/bluez/cosmic_applet_bluetooth/agent";

    /// Error returned to BlueZ.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ReqError {
        Rejected,
        Canceled,
    }

    pub type ReqResult<T> = std::result::Result<T, ReqError>;

    type ReqFn<A, R> =
        Box<dyn Fn(A) -> Pin<Box<dyn Future<Output = ReqResult<R>> + Send>> + Send + Sync>;

    pub struct RequestPinCode {
        pub device: Address,
    }

    pub struct DisplayPinCode {
        pub device: Address,
        pub pincode: String,
    }

    pub struct RequestPasskey {
        pub device: Address,
    }

    pub struct DisplayPasskey {
        pub device: Address,
        pub passkey: u32,
    }

    pub struct RequestConfirmation {
        pub device: Address,
        pub passkey: u32,
    }

    pub struct RequestAuthorization {
        pub device: Address,
    }

    pub struct AuthorizeService {
        pub device: Address,
        pub service: String,
    }

    #[derive(Default)]
    pub struct Agent {
        pub request_default: bool,
        pub request_pin_code: Option<ReqFn<RequestPinCode, String>>,
        pub display_pin_code: Option<ReqFn<DisplayPinCode, ()>>,
        pub request_passkey: Option<ReqFn<RequestPasskey, u32>>,
        pub display_passkey: Option<ReqFn<DisplayPasskey, ()>>,
        pub request_confirmation: Option<ReqFn<RequestConfirmation, ()>>,
        pub request_authorization: Option<ReqFn<RequestAuthorization, ()>>,
        pub authorize_service: Option<ReqFn<AuthorizeService, ()>>,
        pub _non_exhaustive: (),
    }

    impl Agent {
        fn capability(&self) -> &'static str {
            let keyboard = self.request_passkey.is_some() || self.request_pin_code.is_some();
            let display_only = self.display_passkey.is_some() || self.display_pin_code.is_some();
            let yes_no = self.request_confirmation.is_some()
                || self.request_authorization.is_some()
                || self.authorize_service.is_some();
            match (keyboard, display_only, yes_no) {
                (true, false, false) => "KeyboardOnly",
                (false, true, false) => "DisplayOnly",
                (false, _, true) => "DisplayYesNo",
                (true, true, _) | (true, _, true) => "KeyboardDisplay",
                (false, false, false) => "NoInputNoOutput",
            }
        }
    }

    #[derive(Debug, zbus::DBusError)]
    #[zbus(prefix = "org.bluez.Error")]
    enum AgentError {
        #[zbus(error)]
        ZBus(zbus::Error),
        Rejected,
        Canceled,
    }

    impl From<ReqError> for AgentError {
        fn from(error: ReqError) -> Self {
            match error {
                ReqError::Rejected => Self::Rejected,
                ReqError::Canceled => Self::Canceled,
            }
        }
    }

    struct AgentObject {
        agent: Agent,
        cancel: Mutex<Option<oneshot::Sender<()>>>,
    }

    impl AgentObject {
        /// A new request replaces the cancel slot, which cancels the pending one.
        async fn cancel_receiver(&self) -> oneshot::Receiver<()> {
            let (tx, rx) = oneshot::channel();
            *self.cancel.lock().await = Some(tx);
            rx
        }

        async fn call<A, R>(f: &Option<ReqFn<A, R>>, arg: A) -> ReqResult<R> {
            match f {
                Some(f) => f(arg).await,
                None => Err(ReqError::Rejected),
            }
        }

        async fn call_with_cancel<A, R>(&self, f: &Option<ReqFn<A, R>>, arg: A) -> ReqResult<R> {
            let cancel = self.cancel_receiver().await;
            match f {
                Some(f) => tokio::select! {
                    result = f(arg) => result,
                    _ = cancel => Err(ReqError::Canceled),
                },
                None => Err(ReqError::Rejected),
            }
        }
    }

    fn parse(device: &OwnedObjectPath) -> ReqResult<Address> {
        match parse_device_path(device.as_str()) {
            Some((_, address)) => Ok(address),
            None => {
                tracing::error!("Cannot parse device path {}", device.as_str());
                Err(ReqError::Rejected)
            }
        }
    }

    #[zbus::interface(name = "org.bluez.Agent1")]
    impl AgentObject {
        async fn release(&self) {}

        async fn cancel(&self) {
            if let Some(tx) = self.cancel.lock().await.take() {
                _ = tx.send(());
            }
        }

        async fn request_pin_code(&self, device: OwnedObjectPath) -> Result<String, AgentError> {
            let device = parse(&device)?;
            let request = RequestPinCode { device };
            Ok(self
                .call_with_cancel(&self.agent.request_pin_code, request)
                .await?)
        }

        async fn display_pin_code(
            &self,
            device: OwnedObjectPath,
            pincode: String,
        ) -> Result<(), AgentError> {
            let device = parse(&device)?;
            let _cancel = self.cancel_receiver().await;
            let request = DisplayPinCode { device, pincode };
            Ok(Self::call(&self.agent.display_pin_code, request).await?)
        }

        async fn request_passkey(&self, device: OwnedObjectPath) -> Result<u32, AgentError> {
            let device = parse(&device)?;
            let request = RequestPasskey { device };
            Ok(self
                .call_with_cancel(&self.agent.request_passkey, request)
                .await?)
        }

        async fn display_passkey(
            &self,
            device: OwnedObjectPath,
            passkey: u32,
            _entered: u16,
        ) -> Result<(), AgentError> {
            let device = parse(&device)?;
            let _cancel = self.cancel_receiver().await;
            let request = DisplayPasskey { device, passkey };
            Ok(Self::call(&self.agent.display_passkey, request).await?)
        }

        async fn request_confirmation(
            &self,
            device: OwnedObjectPath,
            passkey: u32,
        ) -> Result<(), AgentError> {
            let device = parse(&device)?;
            let request = RequestConfirmation { device, passkey };
            Ok(self
                .call_with_cancel(&self.agent.request_confirmation, request)
                .await?)
        }

        async fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), AgentError> {
            let device = parse(&device)?;
            let request = RequestAuthorization { device };
            Ok(self
                .call_with_cancel(&self.agent.request_authorization, request)
                .await?)
        }

        async fn authorize_service(
            &self,
            device: OwnedObjectPath,
            uuid: String,
        ) -> Result<(), AgentError> {
            let device = parse(&device)?;
            let request = AuthorizeService {
                device,
                service: uuid,
            };
            Ok(self
                .call_with_cancel(&self.agent.authorize_service, request)
                .await?)
        }
    }

    /// Unregisters the agent when dropped.
    #[must_use = "AgentHandle must be held for agent to be registered"]
    pub struct AgentHandle {
        connection: zbus::Connection,
        path: OwnedObjectPath,
    }

    pub(super) async fn register(
        connection: zbus::Connection,
        agent: Agent,
    ) -> zbus::Result<AgentHandle> {
        let path = OwnedObjectPath::try_from(PATH)?;
        let capability = agent.capability();
        let request_default = agent.request_default;
        let object = AgentObject {
            agent,
            cancel: Mutex::new(None),
        };
        connection.object_server().at(path.clone(), object).await?;

        let manager = AgentManager1Proxy::new(&connection).await?;
        manager.register_agent(&path.as_ref(), capability).await?;
        let handle = AgentHandle {
            connection,
            path: path.clone(),
        };
        if request_default {
            manager.request_default_agent(&path.as_ref()).await?;
        }
        Ok(handle)
    }

    impl Drop for AgentHandle {
        fn drop(&mut self) {
            let connection = self.connection.clone();
            let path = self.path.clone();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    if let Ok(manager) = AgentManager1Proxy::new(&connection).await {
                        _ = manager.unregister_agent(&path.as_ref()).await;
                    }
                    _ = connection
                        .object_server()
                        .remove::<AgentObject, _>(path)
                        .await;
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_round_trip() {
        let address: Address = "aa:BB:0c:dd:ee:01".parse().unwrap();
        assert_eq!(address.to_string(), "AA:BB:0C:DD:EE:01");
        assert!("AA:BB:CC:DD:EE".parse::<Address>().is_err());
        assert!("AA:BB:CC:DD:EE:FF:00".parse::<Address>().is_err());
    }

    #[test]
    fn device_paths() {
        let address: Address = "AA:BB:CC:DD:EE:01".parse().unwrap();
        let path = device_path("hci0", address).unwrap();
        assert_eq!(path.as_str(), "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_01");
        assert_eq!(parse_device_path(path.as_str()), Some(("hci0", address)));
        assert_eq!(parse_device_path("/org/bluez/hci0"), None);
        assert_eq!(
            parse_device_path("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_01/sep1"),
            None
        );
    }
}
