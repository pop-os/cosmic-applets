// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    ffi::{OsStr, OsString},
    fmt::{self, Debug},
    hash::Hash,
    io,
    os::fd::{AsFd, AsRawFd},
    path::{Path, PathBuf},
    time::Duration,
};

use cosmic::iced::{self, subscription};
use drm::control::Device as ControlDevice;
use futures::{FutureExt, SinkExt};
use tokio::{
    io::unix::AsyncFd,
    task::spawn_blocking,
    time::{self, Interval},
};
use tracing::{debug, info, trace};
use udev::EventType;

pub struct GpuMonitor {
    primary_gpu: PathBuf,
    gpus: Vec<Gpu>,
    monitor: AsyncFd<WrappedSocket>,
    seat: String,
}

struct WrappedSocket(udev::MonitorSocket);
impl AsRawFd for WrappedSocket {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        self.0.as_raw_fd()
    }
}
unsafe impl Send for WrappedSocket {}
unsafe impl Sync for WrappedSocket {}

impl Debug for GpuMonitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuMonitor")
            .field("primary_gpu", &self.primary_gpu)
            .field("gpus", &self.gpus)
            .field("monitor", &"...")
            .field("seat", &self.seat)
            .finish()
    }
}

#[derive(Debug)]
struct Gpu {
    path: PathBuf,
    name: String,
    primary: bool,
    enabled: bool,
    driver: Option<OsString>,
    interval: Interval,
}

async fn is_desktop() -> bool {
    let chassis = tokio::fs::read_to_string("/sys/class/dmi/id/chassis_type")
        .await
        .unwrap_or_default();

    chassis.trim() == "3"
}

async fn powered_on(path: impl AsRef<Path>) -> bool {
    let Some(component) = path.as_ref().components().last() else {
        return true;
    };
    let name_str = component.as_os_str();
    let Some(name) = name_str.to_str() else {
        return true;
    };
    let Ok(state) =
        tokio::fs::read_to_string(format!("/sys/class/drm/{}/device/power_state", name)).await
    else {
        return true;
    };

    match state.trim() {
        "D0" => true,
        "D3cold" | "D3hot" => false,
        x => {
            debug!(
                "Unknown power state {} for node {}",
                x,
                path.as_ref().display()
            );
            true
        }
    }
}

impl GpuMonitor {
    async fn new() -> Option<GpuMonitor> {
        if is_desktop().await {
            info!("Desktop, skipping dGPU code");
            return None;
        }

        let seat = std::env::var("XDG_SEAT").unwrap_or_else(|_| String::from("seat0"));
        let seat_clone = seat.clone();
        let gpus = spawn_blocking(move || all_gpus(seat)).await.ok()?.ok()?;

        let monitor = AsyncFd::new(WrappedSocket(
            udev::MonitorBuilder::new()
                .ok()?
                .match_subsystem("drm")
                .ok()?
                .listen()
                .ok()?,
        ))
        .ok()?;

        let primary_gpu = gpus
            .iter()
            .find_map(|gpu| gpu.primary.then(|| gpu.path.clone()))?;

        Some(GpuMonitor {
            primary_gpu,
            gpus,
            monitor,
            seat: seat_clone,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub icon: Option<String>,
    pub secondary: String,
}

#[derive(Debug)]
pub struct RunningApp {
    name: String,
    icon: Option<String>,
    executable_name: String,
}

impl Gpu {
    async fn connected_outputs(&self) -> Option<Vec<Entry>> {
        let path = self.path.clone();
        spawn_blocking(move || {
            struct Device(std::fs::File);
            impl AsFd for Device {
                fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
                    self.0.as_fd()
                }
            }
            impl drm::Device for Device {}
            impl ControlDevice for Device {}

            let device = Device(std::fs::File::open(path).ok()?);
            let resources = device.resource_handles().ok()?;

            let outputs = resources
                .connectors
                .into_iter()
                .filter_map(|conn| device.get_connector(conn, false).ok())
                .filter(|info| info.state() == drm::control::connector::State::Connected)
                .map(|info| Entry {
                    name: format!(
                        "Output @ {}:{}",
                        info.interface().as_str(),
                        info.interface_id()
                    ),
                    icon: Some("display-symbolic".to_string()),
                    secondary: String::new(),
                })
                .collect();
            // TODO read and parse edid with libdisplay-info and display output manufacture/model

            Some(outputs)
        })
        .await
        .ok()?
    }

    async fn app_list(&self, running_apps: &[RunningApp]) -> Option<Vec<Entry>> {
        match self.driver.as_ref().and_then(|s| s.to_str()) {
            Some("nvidia") => {
                // figure out bus path for calling nvidia-smi
                let mut sys_path = PathBuf::from("/sys/class/drm");
                sys_path.push(self.path.components().last()?.as_os_str());
                let buslink = std::fs::read_link(sys_path)
                    .ok()?
                    .components()
                    .rev()
                    .nth(2)?
                    .as_os_str()
                    .to_string_lossy()
                    .into_owned();

                let smi_output = match tokio::process::Command::new("nvidia-smi")
                    .args(["pmon", "--id", &buslink, "--count", "1"])
                    .output()
                    .await
                {
                    Ok(output) if output.status.success() => {
                        String::from_utf8_lossy(&output.stdout).into_owned()
                    }
                    Ok(output) => {
                        debug!(
                            "smi returned error code {}: {}",
                            output.status,
                            String::from_utf8_lossy(&output.stdout)
                        );
                        return None;
                    }
                    Err(err) => {
                        debug!("smi returned error code: {}", err);
                        return None;
                    }
                };

                Some(
                    smi_output
                        .lines()
                        .filter(|line| !line.starts_with('#'))
                        .map(|line| {
                            let components = line.split_whitespace().collect::<Vec<_>>();
                            let pid = components[1].trim();
                            let process_name = components.last().unwrap().trim();

                            if let Some(application) = running_apps
                                .iter()
                                .find(|running_app| running_app.executable_name == process_name)
                            {
                                Entry {
                                    name: application.name.clone(),
                                    icon: application.icon.clone(),
                                    secondary: String::new(),
                                }
                            } else {
                                Entry {
                                    name: process_name.to_string(),
                                    icon: None,
                                    secondary: pid.to_string(),
                                }
                            }
                        })
                        .collect(),
                )
            }
            _ => {
                let lsof_output = match tokio::process::Command::new("lsof")
                    .args([OsStr::new("-t"), self.path.as_os_str()])
                    .output()
                    .await
                {
                    Ok(output) => String::from_utf8_lossy(&output.stdout).into_owned(),
                    Err(err) => {
                        debug!("lsof returned error code: {}", err);
                        return None;
                    }
                };

                Some(
                    lsof_output
                        .lines()
                        .filter_map(|pid| {
                            let executable = std::fs::read_link(format!("/proc/{}/exe", pid))
                                .ok()?
                                .components()
                                .last()?
                                .as_os_str()
                                .to_string_lossy()
                                .into_owned();

                            if let Some(application) = running_apps
                                .iter()
                                .find(|running_app| running_app.executable_name == executable)
                            {
                                Some(Entry {
                                    name: application.name.clone(),
                                    icon: application.icon.clone(),
                                    secondary: String::new(),
                                })
                            } else {
                                Some(Entry {
                                    name: executable,
                                    icon: None,
                                    secondary: pid.to_string(),
                                })
                            }
                        })
                        .collect(),
                )
            }
        }
    }
}

fn all_gpus<S: AsRef<str>>(seat: S) -> io::Result<Vec<Gpu>> {
    let mut enumerator = udev::Enumerator::new()?;
    enumerator.match_subsystem("drm")?;
    enumerator.match_sysname("card[0-9]*")?;
    Ok(enumerator
        .scan_devices()?
        .filter(|device| {
            device
                .property_value("ID_SEAT")
                .map(|x| x.to_os_string())
                .unwrap_or_else(|| OsString::from("seat0"))
                == *seat.as_ref()
        })
        .flat_map(|device| {
            let path = device.devnode().map(PathBuf::from)?;
            let boot_vga = if let Ok(Some(pci)) = device.parent_with_subsystem(Path::new("pci")) {
                if let Some(value) = pci.attribute_value("boot_vga") {
                    value == "1"
                } else {
                    false
                }
            } else {
                false
            };

            let name = if let Some(parent) = device.parent() {
                let vendor = parent
                    .property_value("SWITCHEROO_CONTROL_VENDOR_NAME")
                    .or_else(|| parent.property_value("ID_VENDOR_FROM_DATABASE"));
                let name = parent
                    .property_value("SWITCHEROO_CONTROL_PRODUCT_NAME")
                    .or_else(|| parent.property_value("ID_MODEL_FROM_DATABASE"));

                if vendor.is_none() && name.is_none() {
                    String::from("Unknown GPU")
                } else {
                    format!(
                        "{} {}",
                        vendor.map(|s| s.to_string_lossy()).unwrap_or_default(),
                        name.map(|s| s.to_string_lossy()).unwrap_or_default()
                    )
                }
            } else {
                String::from("Unknown GPU")
            };

            let mut device = Some(device);
            let driver = loop {
                if let Some(dev) = device {
                    if dev.driver().is_some() {
                        break dev.driver().map(std::ffi::OsStr::to_os_string);
                    } else {
                        device = dev.parent();
                    }
                } else {
                    break None;
                }
            };

            let mut interval = time::interval(Duration::from_secs(3));
            interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

            Some(Gpu {
                path,
                name,
                primary: boot_vga,
                enabled: false,
                driver,
                interval,
            })
        })
        .collect())
}

pub fn dgpu_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced::Subscription<GpuUpdate> {
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
    Waiting(GpuMonitor),
    Finished,
}

#[derive(Debug)]
pub enum GpuUpdate {
    Off(PathBuf),
    On(PathBuf, String, Option<Vec<Entry>>),
}

async fn start_listening(
    state: State,
    output: &mut futures::channel::mpsc::Sender<GpuUpdate>,
) -> State {
    match state {
        State::Ready => match GpuMonitor::new().await {
            Some(monitor) => State::Waiting(monitor),
            None => State::Finished,
        },
        State::Waiting(mut monitor) => {
            let select_all = futures::future::select_all(
                monitor
                    .gpus
                    .iter_mut()
                    .map(|gpu| Box::pin(gpu.interval.tick())),
            )
            .map(|(_, i, _)| i);

            tokio::select! {
                guard = monitor.monitor.readable() => {
                    if let Ok(mut guard) = guard {
                        for event in monitor.monitor.get_ref().0.iter() {
                            match event.event_type() {
                                // New device
                                EventType::Add => {
                                    if let Some(path) = event.devnode() {
                                        let device = event.device();
                                        let name = if let Some(parent) = device.parent() {
                                            let vendor = parent
                                                .property_value("SWITCHEROO_CONTROL_VENDOR_NAME")
                                                .or_else(|| parent.property_value("ID_VENDOR_FROM_DATABASE"));
                                            let name = parent
                                                .property_value("SWITCHEROO_CONTROL_PRODUCT_NAME")
                                                .or_else(|| parent.property_value("ID_MODEL_FROM_DATABASE"));

                                            if vendor.is_none() && name.is_none() {
                                                String::from("Unknown GPU")
                                            } else {
                                                format!(
                                                    "{} {}",
                                                    vendor.map(|s| s.to_string_lossy()).unwrap_or_default(),
                                                    name.map(|s| s.to_string_lossy()).unwrap_or_default()
                                                )
                                            }
                                        } else {
                                            String::from("Unknown GPU")
                                        };

                                        let mut device = Some(device);
                                        let driver = loop {
                                            if let Some(dev) = device {
                                                if dev.driver().is_some() {
                                                    break dev.driver().map(std::ffi::OsStr::to_os_string);
                                                } else {
                                                    device = dev.parent();
                                                }
                                            } else {
                                                break None;
                                            }
                                        };

                                        let mut interval = time::interval(Duration::from_secs(3));
                                        interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
                                        monitor.gpus.push(Gpu {
                                            path: path.to_path_buf(),
                                            name,
                                            primary: false,
                                            enabled: false,
                                            driver,
                                            interval,
                                        });
                                    }
                                },
                                EventType::Change => {
                                    if let Some(path) = event.devnode() {
                                        if let Some(gpu) = monitor.gpus.iter_mut().find(|gpu| gpu.path == path) {
                                            gpu.interval.reset_immediately();
                                        }
                                    }
                                }
                                EventType::Remove => {
                                    if let Some(path) = event.devnode() {
                                        monitor.gpus.retain(|gpu| gpu.path != path);
                                    }
                                }
                                _ => {},
                            }
                        }

                        guard.clear_ready_matching(tokio::io::Ready::READABLE);
                    } else {
                        return State::Finished;
                    }
                }
                i = select_all => {
                    let gpu = &mut monitor.gpus[i];
                    if gpu.path == monitor.primary_gpu {
                        return State::Waiting(monitor);
                    }

                    trace!("Polling gpu {}", gpu.path.display());
                    let enabled = powered_on(&gpu.path).await;

                    if enabled != gpu.enabled {
                        let mut new_interval = time::interval(Duration::from_secs(if enabled { 30 } else { 3 }));
                        new_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
                        gpu.interval = new_interval;
                        gpu.enabled = enabled;
                    }

                    if enabled {
                        let mut list = gpu.connected_outputs().await.unwrap_or_default();
                        if let Some(mut apps) = gpu.app_list(&[]).await {
                            apps.retain(|app| app.name != "cosmic-comp" && app.name != "Xwayland");
                            list.extend(apps);
                        }
                        if output.send(GpuUpdate::On(gpu.path.clone(), gpu.name.clone(), (!list.is_empty()).then_some(list))).await.is_err() {
                            return State::Finished;
                        }
                    } else if output.send(GpuUpdate::Off(gpu.path.clone())).await.is_err() {
                        return State::Finished;
                    }
                }
            };

            State::Waiting(monitor)
        }
        State::Finished => iced::futures::future::pending().await,
    }
}
