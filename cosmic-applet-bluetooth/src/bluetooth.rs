// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    mem,
    sync::{Arc, LazyLock},
    time::Duration,
};

pub use bluer::DeviceProperty;
use bluer::{
    agent::{Agent, AgentHandle},
    Adapter, Address, Session, Uuid,
};

use cosmic::{
    iced::{
        self,
        futures::{SinkExt, StreamExt},
        Subscription,
    },
    iced_futures::stream,
};

use futures::{stream::FuturesUnordered, FutureExt};
use rand::Rng;
use tokio::{
    spawn,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Mutex, RwLock,
    },
    task::JoinHandle,
};

static TICK: LazyLock<RwLock<Duration>> = LazyLock::new(|| RwLock::new(Duration::from_secs(10)));

pub async fn set_tick(duration: Duration) {
    let mut guard = TICK.write().await;
    *guard = duration;
}

pub async fn tick(interval: &mut tokio::time::Interval) {
    let guard = TICK.read().await;
    if *guard != interval.period() {
        *interval = tokio::time::interval(*guard);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    }
    interval.tick().await;
}
// Copied from https://github.com/bluez/bluez/blob/39467578207889fd015775cbe81a3db9dd26abea/src/dbus-common.c#L53
#[inline]
fn device_type_to_icon(device_type: &str) -> &'static str {
    match device_type {
        "computer" => "laptop-symbolic",
        "phone" => "smartphone-symbolic",
        "network-wireless" => "network-wireless-symbolic",
        "audio-headset" => "audio-headset-symbolic",
        "audio-headphones" => "audio-headphones-symbolic",
        "camera-video" => "camera-video-symbolic",
        "audio-card" => "audio-card-symbolic",
        "input-gaming" => "input-gaming-symbolic",
        "input-keyboard" => "input-keyboard-symbolic",
        "input-tablet" => "input-tablet-symbolic",
        "input-mouse" => "input-mouse-symbolic",
        "printer" => "printer-network-symbolic",
        "camera-photo" => "camera-photo-symbolic",
        _ => DEFAULT_DEVICE_ICON,
    }
}

#[inline]
pub fn bluetooth_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced::Subscription<BluerEvent> {
    Subscription::run_with_id(
        id,
        stream::channel(50, move |mut output| async move {
            let mut retry_count = 0u32;

            // Initialize connection.
            let mut session_state = loop {
                if let Ok(session) = Session::new().await {
                    if let Ok(state) = BluerSessionState::new(session).await {
                        break state;
                    }
                }

                retry_count = retry_count.saturating_add(1);
                _ = tokio::time::sleep(Duration::from_millis(
                    2_u64.saturating_pow(retry_count).min(68719476734),
                ))
                .await;
            };

            let state = bluer_state(&session_state.adapter).await;

            // reconnect to paired and trusted devices
            if state.bluetooth_enabled {
                for d in &state.devices {
                    if d.paired_and_trusted() {
                        _ = session_state
                            .req_tx
                            .send(BluerRequest::ConnectDevice(d.address))
                            .await;
                    }
                }
            }
            _ = output
                .send(BluerEvent::Init {
                    sender: session_state.req_tx.clone(),
                    state: state.clone(),
                })
                .await;

            let mut event_handler = async |event| {
                let message = match event {
                    BluerSessionEvent::ChangesProcessed(state) => {
                        BluerEvent::DevicesChanged { state }
                    }

                    BluerSessionEvent::RequestResponse {
                        req,
                        state,
                        err_msg,
                    } => BluerEvent::RequestResponse {
                        req,
                        state,
                        err_msg,
                    },

                    BluerSessionEvent::AgentEvent(e) => BluerEvent::AgentEvent(e),

                    _ => return,
                };

                _ = output.send(message).await;
            };

            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                let Some(mut session_rx) = session_state.rx.take() else {
                    break;
                };

                if let Some(event) = session_rx.recv().await {
                    event_handler(event).await;
                    // Consume any additional available events.
                    let mut count = 0;
                    while let Some(event) = session_rx.try_recv().ok() {
                        event_handler(event).await;
                        count += 1;
                        if count == 100 {
                            break;
                        }
                    }
                } else {
                    break;
                };

                session_state.rx = Some(session_rx);
                interval.tick().await;
            }

            _ = output.send(BluerEvent::Finished).await;
            futures::future::pending().await
        }),
    )
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum BluerRequest {
    SetBluetoothEnabled(bool),
    PairDevice(Address),
    ConnectDevice(Address),
    DisconnectDevice(Address),
    CancelConnect(Address),
    StateUpdate,
}

#[derive(Debug, Clone)]
pub enum BluerEvent {
    RequestResponse {
        req: BluerRequest,
        state: BluerState,
        err_msg: Option<String>,
    },
    Init {
        sender: Sender<BluerRequest>,
        state: BluerState,
    },
    DevicesChanged {
        state: BluerState,
    },
    AgentEvent(BluerAgentEvent),
    Finished,
}

#[derive(Debug, Clone, Default)]
pub struct BluerState {
    pub devices: Vec<BluerDevice>,
    pub bluetooth_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BluerDeviceStatus {
    Connected,
    Connecting,
    Paired,
    /// Pairing is in progress, maybe with a passkey or pincode
    /// passkey or pincode will be 000000 - 999999
    Pairing,
    Disconnected,
    Disconnecting,
}

#[derive(Debug, Clone)]
pub struct BluerDevice {
    pub name: String,
    pub icon: &'static str,
    pub address: Address,
    pub status: BluerDeviceStatus,
    pub battery_percent: Option<u8>,
    pub is_paired: bool,
    pub is_trusted: bool,
}

impl Eq for BluerDevice {}

impl Ord for BluerDevice {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.status.cmp(&other.status) {
            std::cmp::Ordering::Equal => self.name.to_lowercase().cmp(&other.name.to_lowercase()),
            o => o,
        }
    }
}

impl PartialOrd for BluerDevice {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.status.cmp(&other.status) {
            std::cmp::Ordering::Equal => {
                Some(self.name.to_lowercase().cmp(&other.name.to_lowercase()))
            }
            o => Some(o),
        }
    }
}

impl PartialEq for BluerDevice {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.address == other.address
    }
}

const DEFAULT_DEVICE_ICON: &str = "bluetooth-symbolic";

impl BluerDevice {
    #[inline(never)]
    pub async fn from_device(device: &bluer::Device) -> Self {
        let (mut name, is_paired, is_trusted, is_connected, battery_percent, icon) = futures::join!(
            device.name().map(|res| res
                .ok()
                .flatten()
                .unwrap_or_else(|| device.address().to_string())),
            device.is_paired().map(Result::unwrap_or_default),
            device.is_trusted().map(Result::unwrap_or_default),
            device.is_connected().map(Result::unwrap_or_default),
            device.battery_percentage().map(|res| res.ok().flatten()),
            device
                .icon()
                .map(|res| device_type_to_icon(&res.ok().flatten().unwrap_or_default()))
        );

        if name.is_empty() {
            name = device.address().to_string();
        };

        let status = if is_connected {
            BluerDeviceStatus::Connected
        } else if is_paired {
            BluerDeviceStatus::Paired
        } else {
            BluerDeviceStatus::Disconnected
        };

        Self {
            name,
            icon,
            address: device.address(),
            status,
            battery_percent,
            is_paired,
            is_trusted,
        }
    }

    #[inline]
    fn paired_and_trusted(&self) -> bool {
        self.is_paired && self.is_trusted
    }

    #[inline]
    #[must_use]
    pub fn is_known_device_type(&self) -> bool {
        self.icon != DEFAULT_DEVICE_ICON
    }

    #[inline]
    #[must_use]
    pub fn has_name(&self) -> bool {
        self.name != self.address.to_string()
    }
}

#[derive(Debug, Clone)]
pub enum BluerSessionEvent {
    RequestResponse {
        req: BluerRequest,
        state: BluerState,
        err_msg: Option<String>,
    },
    ChangesProcessed(BluerState),
    AgentEvent(BluerAgentEvent),
}

#[derive(Debug, Clone)]
pub enum BluerAgentEvent {
    DisplayPinCode(BluerDevice, String),
    DisplayPasskey(BluerDevice, String),
    RequestPinCode(BluerDevice),
    RequestPasskey(BluerDevice),
    RequestConfirmation(BluerDevice, String, Sender<bool>), // Note mpsc channel is used bc the sender must be cloned in the iced Message machinery
    RequestDeviceAuthorization(BluerDevice, Sender<bool>),
    RequestServiceAuthorization(BluerDevice, Uuid, Sender<bool>),
}

pub struct BluerSessionState {
    _session: Session,
    _agent_handle: AgentHandle,
    pub adapter: Adapter,
    pub rx: Option<Receiver<BluerSessionEvent>>,
    pub req_tx: Sender<BluerRequest>,
    wake_up_discover_tx: Sender<()>,
    wake_up_discover_rx: Option<Receiver<()>>,
    tx: Sender<BluerSessionEvent>,
    active_requests: Arc<Mutex<HashMap<BluerRequest, JoinHandle<anyhow::Result<()>>>>>,
}

impl BluerSessionState {
    async fn new(session: Session) -> anyhow::Result<Self> {
        let adapter = session.default_adapter().await?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let (req_tx, req_rx) = channel(100);
        let tx_clone_1 = tx.clone();
        let tx_clone_2 = tx.clone();
        let tx_clone_3 = tx.clone();
        let tx_clone_4 = tx.clone();
        let tx_clone_5 = tx.clone();
        let tx_clone_6 = tx.clone();
        let tx_clone_7 = tx.clone();
        let adapter_clone_1 = adapter.clone();
        let adapter_clone_2 = adapter.clone();
        let adapter_clone_3 = adapter.clone();
        let adapter_clone_4 = adapter.clone();
        let adapter_clone_5 = adapter.clone();
        let adapter_clone_6 = adapter.clone();
        let adapter_clone_7 = adapter.clone();

        let _agent = Agent {
            request_default: false, // TODO which agent should eventually become the default? Maybe the one in the settings app?
            request_pin_code: Some(Box::new(move |req| {
                let agent_clone = adapter_clone_1.clone();
                let tx_clone = tx_clone_1.clone();
                Box::pin(async move {
                    let device = match agent_clone.device(req.device) {
                        Ok(d) => d,
                        Err(_) => return Err(bluer::agent::ReqError::Rejected),
                    };
                    let _ = tx_clone
                        .send(BluerSessionEvent::AgentEvent(
                            BluerAgentEvent::RequestPinCode(
                                BluerDevice::from_device(&device).await,
                            ),
                        ))
                        .await;
                    let mut rng = rand::rng();
                    let pin_code = rng.random_range(0..999999);
                    Ok(format!("{:06}", pin_code))
                })
            })),
            display_pin_code: Some(Box::new(move |req| {
                let agent_clone = adapter_clone_2.clone();
                let tx_clone = tx_clone_2.clone();
                Box::pin(async move {
                    let device = match agent_clone.device(req.device) {
                        Ok(d) => d,
                        Err(_) => return Err(bluer::agent::ReqError::Rejected),
                    };
                    let _ = tx_clone
                        .send(BluerSessionEvent::AgentEvent(
                            BluerAgentEvent::DisplayPinCode(
                                BluerDevice::from_device(&device).await,
                                req.pincode,
                            ),
                        ))
                        .await;

                    Ok(())
                })
            })),
            request_passkey: Some(Box::new(move |req| {
                let agent_clone = adapter_clone_3.clone();
                let tx_clone = tx_clone_3.clone();
                Box::pin(async move {
                    let device = match agent_clone.device(req.device) {
                        Ok(d) => d,
                        Err(_) => return Err(bluer::agent::ReqError::Rejected),
                    };
                    let _ = tx_clone
                        .send(BluerSessionEvent::AgentEvent(
                            BluerAgentEvent::RequestPasskey(
                                BluerDevice::from_device(&device).await,
                            ),
                        ))
                        .await;
                    let mut rng = rand::rng();
                    let pin_code = rng.random_range(0..999999);
                    Ok(pin_code)
                })
            })),
            display_passkey: Some(Box::new(move |req| {
                let agent_clone = adapter_clone_4.clone();
                let tx_clone = tx_clone_4.clone();
                Box::pin(async move {
                    let device = match agent_clone.device(req.device) {
                        Ok(d) => d,
                        Err(_) => return Err(bluer::agent::ReqError::Rejected),
                    };
                    let _ = tx_clone
                        .send(BluerSessionEvent::AgentEvent(
                            BluerAgentEvent::DisplayPasskey(
                                BluerDevice::from_device(&device).await,
                                format!("{:06}", req.passkey),
                            ),
                        ))
                        .await;
                    Ok(())
                })
            })),
            request_confirmation: Some(Box::new(move |req| {
                let agent_clone = adapter_clone_5.clone();
                let tx_clone = tx_clone_5.clone();
                Box::pin(async move {
                    let device = match agent_clone.device(req.device) {
                        Ok(d) => d,
                        Err(_) => return Err(bluer::agent::ReqError::Rejected),
                    };
                    let (tx, mut rx) = channel(1);
                    let _ = tx_clone
                        .send(BluerSessionEvent::AgentEvent(
                            BluerAgentEvent::RequestConfirmation(
                                BluerDevice::from_device(&device).await,
                                format!("{:06}", req.passkey),
                                tx,
                            ),
                        ))
                        .await;
                    let res = rx.recv().await;
                    match res {
                        Some(res) if res => Ok(()),
                        _ => Err(bluer::agent::ReqError::Rejected),
                    }
                })
            })),
            request_authorization: Some(Box::new(move |req| {
                let agent_clone = adapter_clone_6.clone();
                let tx_clone = tx_clone_6.clone();
                Box::pin(async move {
                    let device = match agent_clone.device(req.device) {
                        Ok(d) => d,
                        Err(_) => return Err(bluer::agent::ReqError::Rejected),
                    };
                    let (tx, mut rx) = channel(1);
                    let _ = tx_clone
                        .send(BluerSessionEvent::AgentEvent(
                            BluerAgentEvent::RequestDeviceAuthorization(
                                BluerDevice::from_device(&device).await,
                                tx,
                            ),
                        ))
                        .await;
                    let res = rx.recv().await;
                    match res {
                        Some(res) if res => Ok(()),
                        _ => Err(bluer::agent::ReqError::Rejected),
                    }
                })
            })),
            authorize_service: Some(Box::new(move |req| {
                let agent_clone = adapter_clone_7.clone();
                let tx_clone = tx_clone_7.clone();
                Box::pin(async move {
                    let device = match agent_clone.device(req.device) {
                        Ok(d) => d,
                        Err(_) => return Err(bluer::agent::ReqError::Rejected),
                    };
                    let (tx, mut rx) = channel(1);
                    // TODO better describe the service to the user
                    let _ = tx_clone
                        .send(BluerSessionEvent::AgentEvent(
                            BluerAgentEvent::RequestServiceAuthorization(
                                BluerDevice::from_device(&device).await,
                                req.service,
                                tx,
                            ),
                        ))
                        .await;
                    let res = rx.recv().await;
                    match res {
                        Some(res) if res => Ok(()),
                        _ => Err(bluer::agent::ReqError::Rejected),
                    }
                })
            })),
            _non_exhaustive: (),
        };
        let _agent_handle = session.register_agent(_agent).await?;
        let (wake_up_discover_tx, wake_up_discover_rx) = channel(10);
        let mut self_ = Self {
            _agent_handle,
            _session: session,
            adapter,
            rx: Some(rx),
            req_tx,
            wake_up_discover_rx: Some(wake_up_discover_rx),
            wake_up_discover_tx,
            tx,
            active_requests: Arc::new(Mutex::new(HashMap::new())),
        };
        self_.process_requests(req_rx);
        self_.process_changes();
        self_.listen_bluetooth_power_changes();

        Ok(self_)
    }

    #[inline]
    fn listen_bluetooth_power_changes(&self) {
        let tx = self.tx.clone();
        let req_tx = self.req_tx.clone();
        let adapter_clone = self.adapter.clone();
        let wake_up_discover_tx = self.wake_up_discover_tx.clone();
        let _handle: JoinHandle<anyhow::Result<()>> = spawn(async move {
            let mut status = adapter_clone.is_powered().await.unwrap_or_default();
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut devices = Vec::new();
            loop {
                tick(&mut interval).await;
                let new_status = adapter_clone.is_powered().await.unwrap_or_default();
                devices = build_device_list(devices, &adapter_clone).await;
                if new_status != status {
                    status = new_status;
                    let state = BluerState {
                        devices: devices.clone(),
                        bluetooth_enabled: status,
                    };
                    if state.bluetooth_enabled {
                        for d in &state.devices {
                            if d.paired_and_trusted() {
                                _ = req_tx.send(BluerRequest::ConnectDevice(d.address)).await;
                            }
                        }
                    }
                    _ = wake_up_discover_tx.send(()).await;
                    let _ = tx.send(BluerSessionEvent::ChangesProcessed(state)).await;
                }
            }
        });
    }

    #[inline]
    fn process_changes(&mut self) {
        let tx = self.tx.clone();
        let req_tx = self.req_tx.clone();
        let Some(mut wake_up) = self.wake_up_discover_rx.take() else {
            tracing::error!("Failed to take wake up channel");
            return;
        };
        let adapter_clone = self.adapter.clone();
        let _monitor_devices: tokio::task::JoinHandle<Result<(), anyhow::Error>> =
            spawn(async move {
                let mut devices: Vec<BluerDevice> = Vec::new();
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    interval.tick().await;
                    let wakeup_fut = wake_up.recv();

                    // Listens for process changes and builds edvice lists.
                    let listener_fut = async {
                        let mut new_devices = Vec::new();
                        let mut interval = tokio::time::interval(Duration::from_secs(10));
                        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        let mut change_stream =
                            match adapter_clone.discover_devices_with_changes().await {
                                Ok(stream) => stream,
                                Err(_) => {
                                    tick(&mut interval).await;
                                    return;
                                }
                            };

                        while let Some(_) = change_stream.next().await {
                            new_devices = build_device_list(new_devices, &adapter_clone).await;
                            for d in new_devices
                                .iter()
                                .filter(|d| !devices.contains(d) && d.paired_and_trusted())
                            {
                                _ = req_tx.send(BluerRequest::ConnectDevice(d.address)).await;
                            }

                            let _ = tx
                                .send(BluerSessionEvent::ChangesProcessed(BluerState {
                                    devices: new_devices.clone(),
                                    bluetooth_enabled: adapter_clone
                                        .is_powered()
                                        .await
                                        .unwrap_or_default(),
                                }))
                                .await;

                            devices.clear();
                            mem::swap(&mut new_devices, &mut devices);
                            interval.tick().await;
                        }
                    };

                    futures::pin_mut!(listener_fut);
                    futures::pin_mut!(wakeup_fut);

                    futures::future::select(listener_fut, wakeup_fut).await;
                }
            });
    }

    #[inline]
    fn process_requests(&self, request_rx: Receiver<BluerRequest>) {
        let active_requests = self.active_requests.clone();
        let adapter = self.adapter.clone();
        let tx = self.tx.clone();
        let wake_up_tx = self.wake_up_discover_tx.clone();

        let _handle: JoinHandle<anyhow::Result<()>> = spawn(async move {
            let mut request_rx = request_rx;

            while let Some(req) = request_rx.recv().await {
                let req_clone = req.clone();
                let req_clone_2 = req.clone();
                let active_requests_clone = active_requests.clone();
                let tx_clone = tx.clone();
                let adapter_clone = adapter.clone();
                let wake_up_tx = wake_up_tx.clone();

                let handle = spawn(async move {
                    let mut err_msg = None;
                    match &req_clone {
                        BluerRequest::SetBluetoothEnabled(enabled) => {
                            if let Err(e) = adapter_clone.set_powered(*enabled).await {
                                tracing::error!("Failed to power off bluetooth adapter. {e:?}")
                            }

                            // rfkill will be persisted after reboot
                            let name = adapter_clone.name();
                            if let Some(id) = tokio::process::Command::new("rfkill")
                                .arg("list")
                                .arg("-n")
                                .arg("--output")
                                .arg("ID,DEVICE")
                                .output()
                                .await
                                .ok()
                                .and_then(|o| {
                                    let lines = String::from_utf8(o.stdout).ok()?;
                                    lines.split("\n").into_iter().find_map(|row| {
                                        let (id, cname) = row.trim().split_once(" ")?;
                                        (name == cname).then_some(id.to_string())
                                    })
                                })
                            {
                                if let Err(err) = tokio::process::Command::new("rfkill")
                                    .arg(if *enabled { "unblock" } else { "block" })
                                    .arg(id)
                                    .output()
                                    .await
                                {
                                    tracing::error!(
                                        "Failed to set bluetooth state using rfkill. {err:?}"
                                    );
                                }
                            }

                            if *enabled {
                                _ = wake_up_tx.send(()).await;
                            }
                        }
                        BluerRequest::PairDevice(address) => {
                            let res = adapter_clone.device(*address);
                            if let Err(err) = res {
                                err_msg = Some(err.to_string());
                            } else if let Ok(device) = res {
                                let res = device.pair().await;
                                if let Err(err) = res {
                                    err_msg = Some(err.to_string());
                                } else {
                                    if let Err(err) = device.set_trusted(true).await {
                                        tracing::error!(?err, "Failed to trust device.");
                                    }
                                }
                            }
                        }
                        BluerRequest::ConnectDevice(address) => {
                            let res = adapter_clone.device(*address);
                            if let Err(err) = res {
                                err_msg = Some(err.to_string());
                            } else if let Ok(device) = res {
                                let res = device.connect().await;
                                if let Err(err) = res {
                                    err_msg = Some(err.to_string());
                                } else {
                                    if let Err(err) = device.set_trusted(true).await {
                                        tracing::error!(?err, "Failed to trust device.");
                                    }
                                }
                            }
                        }
                        BluerRequest::DisconnectDevice(address) => {
                            let res = adapter_clone.device(*address);
                            if let Err(err) = res {
                                err_msg = Some(err.to_string());
                            } else if let Ok(device) = res {
                                let res = device.disconnect().await;
                                if let Err(err) = res {
                                    err_msg = Some(err.to_string());
                                }
                            }
                        }
                        BluerRequest::CancelConnect(_) => {
                            if let Some(handle) = active_requests_clone.lock().await.get(&req_clone)
                            {
                                handle.abort();
                            } else {
                                err_msg = Some("No active connection request found".to_string());
                            }
                        }
                        BluerRequest::StateUpdate => {}
                    };

                    let _ = tx_clone
                        .send(BluerSessionEvent::RequestResponse {
                            req: req_clone,
                            state: bluer_state(&adapter_clone).await,
                            err_msg,
                        })
                        .await;

                    active_requests_clone.lock().await.remove(&req_clone_2);

                    Ok(())
                });

                active_requests.lock().await.insert(req, handle);
            }
            Ok(())
        });
    }
}

#[inline]
async fn bluer_state(adapter: &Adapter) -> BluerState {
    let (devices, bluetooth_enabled) = futures::join!(
        build_device_list(Vec::new(), adapter),
        // TODO is this a proper way of checking if bluetooth is enabled?
        adapter.is_powered().map(Result::unwrap_or_default),
    );

    BluerState {
        devices,
        bluetooth_enabled,
    }
}

#[inline(never)]
async fn build_device_list(mut devices: Vec<BluerDevice>, adapter: &Adapter) -> Vec<BluerDevice> {
    let addrs = adapter.device_addresses().await.unwrap_or_default();
    devices.clear();
    if addrs.len() > devices.capacity() {
        devices.reserve(addrs.len() - devices.capacity());
    }

    // Concurrently collect bluer devices from each address.
    let mut device_stream = addrs
        .into_iter()
        .filter_map(|address| adapter.device(address).ok())
        .map(async move |device| BluerDevice::from_device(&device).await)
        .collect::<FuturesUnordered<_>>();

    while let Some(device) = device_stream.next().await {
        devices.push(device)
    }

    devices.sort();
    devices
}
