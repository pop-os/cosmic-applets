use std::{collections::HashMap, fmt::Debug, hash::Hash, sync::Arc, time::Duration};

use bluer::{Adapter, AdapterProperty, Address, DeviceProperty, Session};
use cosmic::iced::{self, subscription};

use futures::StreamExt;
use tokio::{
    spawn,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Mutex,
    },
    task::JoinHandle,
    time::timeout,
};

pub fn bluetooth_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced::Subscription<(I, BluerEvent)> {
    subscription::unfold(id, State::Ready, move |state| start_listening(id, state))
}

#[derive(Debug)]
pub enum State {
    Ready,
    Waiting { session_state: BluerSessionState },
    Finished,
}

async fn start_listening<I: Copy + Debug>(id: I, state: State) -> (Option<(I, BluerEvent)>, State) {
    match state {
        State::Ready => {
            let session = match Session::new().await {
                Ok(s) => s,
                Err(_) => return (None, State::Finished),
            };
            let (tx, rx) = channel(100);

            let session_state = match BluerSessionState::new(session, rx).await {
                Ok(s) => s,
                Err(_) => return (None, State::Finished),
            };

            let state = session_state.bluer_state().await;
            return (
                Some((
                    id,
                    BluerEvent::Init {
                        sender: tx,
                        state: state.clone(),
                    },
                )),
                State::Waiting { session_state },
            );
        }
        State::Waiting { mut session_state } => {
            let mut session_rx = match session_state.rx.take() {
                Some(rx) => rx,
                None => {
                    // try restarting the stream
                    session_state.process_changes();
                    match session_state.rx.take() {
                        Some(rx) => rx,
                        None => {
                            return (None, State::Finished); // fail if we can't restart the stream
                        }
                    }
                }
            };

            let event = if let Some(event) = session_rx.recv().await {
                match event {
                    BluerSessionEvent::ChangesProcessed(state) => {
                        return (
                            Some((id, BluerEvent::DevicesChanged { state })),
                            State::Waiting { session_state },
                        );
                    }
                    BluerSessionEvent::RequestResponse {
                        req,
                        state,
                        err_msg,
                    } => Some((
                        id,
                        BluerEvent::RequestResponse {
                            req,
                            state,
                            err_msg,
                        },
                    )),
                    _ => None,
                }
            } else {
                return (None, State::Finished);
            };

            session_state.rx = Some(session_rx);
            (event, State::Waiting { session_state })
        }
        State::Finished => iced::futures::future::pending().await,
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum BluerRequest {
    SetBluetoothEnabled(bool),
    PairDevice(Address),
    ConnectDevice(Address),
    DisconnectDevice(Address),
    CancelConnect(Address),
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
    Finished,
}

#[derive(Debug, Clone, Default)]
pub struct BluerState {
    pub devices: Vec<BluerDevice>,
    pub bluetooth_enabled: bool,
}

#[derive(Debug, Clone)]
pub enum BluerDeviceStatus {
    Connected,
    Disconnected,
    Paired,
    Connecting,
    Disconnecting,
    Pairing,
}

#[derive(Debug, Clone)]
pub struct BluerDevice {
    pub name: String,
    pub address: Address,
    pub status: BluerDeviceStatus,
    pub properties: Vec<DeviceProperty>,
}

pub enum BluerSessionEvent {
    RequestResponse {
        req: BluerRequest,
        state: BluerState,
        err_msg: Option<String>,
    },
    ChangesProcessed(BluerState),
    ChangeStreamEnded, // TODO can we just restart the stream in a new task?
}

#[derive(Debug)]
pub struct BluerSessionState {
    session: Session,
    pub adapter: Adapter,
    pub devices: Arc<Mutex<Vec<BluerDevice>>>,
    pub rx: Option<Receiver<BluerSessionEvent>>,
    tx: Option<Sender<BluerSessionEvent>>,
    active_requests: Arc<Mutex<HashMap<BluerRequest, JoinHandle<anyhow::Result<()>>>>>,
}

impl BluerSessionState {
    pub(crate) async fn new(
        session: Session,
        request_rx: Receiver<BluerRequest>,
    ) -> anyhow::Result<Self> {
        let adapter = session.default_adapter().await?;
        let devices = build_device_list(&adapter).await;

        let mut self_ = Self {
            session,
            adapter: adapter,
            devices: Arc::new(Mutex::new(devices)),
            rx: None,
            tx: None,
            active_requests: Arc::new(Mutex::new(HashMap::new())),
        };
        self_.process_changes();
        self_.process_requests(request_rx);

        Ok(self_)
    }

    pub(crate) async fn devices(&self) -> Vec<BluerDevice> {
        self.devices.lock().await.clone()
    }

    pub(crate) async fn clear(&mut self) {
        self.devices.lock().await.clear();
    }

    pub(crate) fn start_monitoring(&mut self) {
        self.process_changes();
    }

    pub(crate) fn process_changes(&mut self) {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        self.tx = Some(tx.clone());
        let devices_clone = self.devices.clone();
        let adapter_clone = self.adapter.clone();
        let _monitor_devices: tokio::task::JoinHandle<Result<(), anyhow::Error>> =
            spawn(async move {
                let mut change_stream = adapter_clone.discover_devices_with_changes().await?;

                let mut cur = None;
                let mut devices_changed = false;
                let mut milli_timeout = 10;
                'outer: loop {
                    while let Ok(event) =
                        timeout(Duration::from_millis(milli_timeout), change_stream.next()).await
                    {
                        let event = match event {
                            Some(e) => e,
                            None => break 'outer, // No more events to receive...
                        };
                        let mut devices = devices_clone.lock().await;
                        match event {
                            bluer::AdapterEvent::DeviceAdded(address) => {
                                let device = match adapter_clone.device(address) {
                                    Ok(d) => d,
                                    Err(_) => continue,
                                };

                                let mut status = if device.is_connected().await? {
                                    BluerDeviceStatus::Connected
                                } else if device.is_paired().await? {
                                    BluerDeviceStatus::Paired
                                } else {
                                    BluerDeviceStatus::Disconnected
                                };

                                if let Some(pos) =
                                    devices.iter().position(|device| device.address == address)
                                {
                                    cur = Some(pos);
                                    continue;
                                };
                                // only send a DevicesChanged event if we have actually added a device
                                devices_changed = true;

                                devices.push(BluerDevice {
                                    name: device
                                        .name()
                                        .await
                                        .unwrap_or_default()
                                        .unwrap_or_default(),
                                    address: device.address(),
                                    status,
                                    properties: Vec::new(),
                                });
                                cur = Some(devices.len() - 1);
                            }
                            bluer::AdapterEvent::DeviceRemoved(address) => {
                                if let Some(pos) =
                                    devices.iter().position(|device| device.address == address)
                                {
                                    devices_changed = true;
                                    cur = None;
                                    devices.remove(pos);
                                };
                            }
                            bluer::AdapterEvent::PropertyChanged(prop) => {
                                let bluer_device = match cur.and_then(|i| devices.get_mut(i)) {
                                    Some(d) => d,
                                    None => continue,
                                };
                                devices_changed = true;
                            }
                        }
                    }
                    if devices_changed {
                        devices_changed = false;
                        dbg!(&devices_clone);
                        let _ = tx
                            .send(BluerSessionEvent::ChangesProcessed(BluerState {
                                devices: build_device_list(&adapter_clone).await,
                                bluetooth_enabled: true,
                            }))
                            .await;
                        // reset timeout
                        milli_timeout = 10;
                    } else {
                        // slow down if no changes occur
                        milli_timeout = (milli_timeout * 2).max(5120);
                    }
                }
                eprintln!("Change stream ended");
                Ok(())
            });
        self.rx.replace(rx);
    }

    pub(crate) fn process_requests(&self, request_rx: Receiver<BluerRequest>) {
        let active_requests = self.active_requests.clone();
        let adapter = self.adapter.clone();
        let devices = self.devices.clone();
        let tx = self.tx.clone().unwrap(); // TODO error handling
        let _handle: JoinHandle<anyhow::Result<()>> = spawn(async move {
            let mut request_rx = request_rx;

            while let Some(req) = request_rx.recv().await {
                let req_clone = req.clone();
                let req_clone_2 = req.clone();
                let active_requests_clone = active_requests.clone();
                let devices_clone = devices.clone();
                let tx_clone = tx.clone();
                let adapter_clone = adapter.clone();
                let handle = spawn(async move {
                    let mut err_msg = None;
                    match &req_clone {
                        BluerRequest::SetBluetoothEnabled(enabled) => {
                            let res = adapter_clone.set_powered(*enabled).await;
                            if let Err(e) = res {
                                err_msg = Some(e.to_string());
                            }
                            if *enabled {
                                let res = adapter_clone.set_discoverable(*enabled).await;
                                if let Err(e) = res {
                                    err_msg = Some(e.to_string());
                                }
                            }
                        }
                        BluerRequest::PairDevice(address) => {
                            let res = adapter_clone.device(address.clone());
                            if let Err(err) = res {
                                err_msg = Some(err.to_string());
                            } else if let Ok(device) = res {
                                let res = device.pair().await;
                                if let Err(err) = res {
                                    err_msg = Some(err.to_string());
                                }
                            }
                        }
                        BluerRequest::ConnectDevice(address) => {
                            let res = adapter_clone.device(address.clone());
                            if let Err(err) = res {
                                err_msg = Some(err.to_string());
                            } else if let Ok(device) = res {
                                let res = device.connect().await;
                                if let Err(err) = res {
                                    err_msg = Some(err.to_string());
                                }
                            }
                        }
                        BluerRequest::DisconnectDevice(address) => {
                            let res = adapter_clone.device(address.clone());
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
                    };

                    let state = BluerState {
                        devices: build_device_list(&adapter_clone).await,
                        bluetooth_enabled: adapter_clone.is_powered().await.unwrap_or_default(),
                    };

                    let _ = tx_clone
                        .send(BluerSessionEvent::RequestResponse {
                            req: req_clone,
                            state,
                            err_msg,
                        })
                        .await;

                    let mut active_requests_clone = active_requests_clone.lock().await;
                    let _ = active_requests_clone.remove(&req_clone_2);

                    Ok(())
                });

                active_requests.lock().await.insert(req, handle);
            }
            Ok(())
        });
    }

    pub(crate) async fn bluer_state(&self) -> BluerState {
        BluerState {
            devices: build_device_list(&self.adapter).await,
            // TODO is this a proper way of checking if bluetooth is enabled?
            bluetooth_enabled: self.adapter.is_powered().await.unwrap_or_default(),
        }
    }
}

async fn build_device_list(adapter: &Adapter) -> Vec<BluerDevice> {
    let addrs = adapter.device_addresses().await.unwrap_or_default();
    let mut devices = Vec::with_capacity(addrs.len());

    for address in addrs {
        let device = match adapter.device(address) {
            Ok(device) => device,
            Err(_) => continue,
        };
        let name = device.name().await.unwrap_or_default().unwrap_or_default();
        let is_paired = device.is_paired().await.unwrap_or_default();
        let is_connected = device.is_connected().await.unwrap_or_default();
        let properties = device.all_properties().await.unwrap_or_default();
        let status = if is_connected {
            BluerDeviceStatus::Connected
        } else if is_paired {
            BluerDeviceStatus::Paired
        } else {
            BluerDeviceStatus::Disconnected
        };
        devices.push(BluerDevice {
            name,
            address,
            status,
            properties,
        });
    }
    devices
}
