// TODO: don't allow brightness 0?
// TODO: handle dbus service start/stop?

use futures::prelude::*;
use gtk4::{glib, prelude::*};
use relm4::{ComponentParts, ComponentSender, RelmApp, SimpleComponent, WidgetPlus};
use std::{process::Command, time::Duration};

mod backlight;
use backlight::{backlight, Backlight, LogindSessionProxy};
mod power_daemon;
use power_daemon::PowerDaemonProxy;
mod upower;
use upower::UPowerProxy;
mod upower_device;
use upower_device::DeviceProxy;
mod upower_kbdbacklight;
use upower_kbdbacklight::KbdBacklightProxy;

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

// XXX improve
// TODO: time to empty varies? needs averaging?
fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs > 60 {
        let min = secs / 60;
        if min > 60 {
            format!("{}:{:02}", min / 60, min % 60)
        } else {
            format!("{}m", min)
        }
    } else {
        format!("{}s", secs)
    }
}

#[derive(Copy, Clone)]
enum Graphics {
    Compute,
    Hybrid,
    Integrated,
    Nvidia,
}

impl Graphics {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "compute" => Some(Self::Compute),
            "hybrid" => Some(Self::Hybrid),
            "integrated" => Some(Self::Integrated),
            "nvidia" => Some(Self::Nvidia),
            _ => None,
        }
    }

    fn to_str(self) -> &'static str {
        match self {
            Self::Compute => "compute",
            Self::Hybrid => "hybrid",
            Self::Integrated => "integrated",
            Self::Nvidia => "nvidia",
        }
    }
}

#[derive(Default)]
struct AppModel {
    icon_name: String,
    battery_percent: f64,
    time_remaining: Duration,
    display_brightness: f64,
    keyboard_brightness: f64,
    device: Option<DeviceProxy<'static>>,
    session: Option<LogindSessionProxy<'static>>,
    backlight: Option<Backlight>,
    kbd_backlight: Option<KbdBacklightProxy<'static>>,
    power_daemon: Option<PowerDaemonProxy<'static>>,
}

enum AppMsg {
    SetDisplayBrightness(f64),
    SetKeyboardBrightness(f64),
    SetDevice(DeviceProxy<'static>),
    SetSession(LogindSessionProxy<'static>),
    SetKbdBacklight(KbdBacklightProxy<'static>),
    SetPowerDaemon(PowerDaemonProxy<'static>),
    UpdateProperties,
    UpdateKbdBrightness(f64),
}

#[relm4::component]
impl SimpleComponent for AppModel {
    type Widgets = AppWidgets;

    type InitParams = ();

    type Input = AppMsg;
    type Output = ();

    view! {
        gtk4::Window {
            gtk4::MenuButton {
                set_has_frame: false,
                #[watch]
                set_icon_name: &model.icon_name,
                #[wrap(Some)]
                set_popover = &gtk4::Popover {
                    #[wrap(Some)]
                    set_child = &gtk4::Box {
                        set_orientation: gtk4::Orientation::Vertical,

                        // Battery
                        gtk4::Box {
                            set_orientation: gtk4::Orientation::Horizontal,
                            gtk4::Image {
                                #[watch]
                                set_icon_name: Some(&model.icon_name),
                            },
                            gtk4::Box {
                                set_orientation: gtk4::Orientation::Vertical,
                                gtk4::Label {
                                    set_halign: gtk4::Align::Start,
                                    set_label: "Battery",
                                },
                                gtk4::Label {
                                    set_halign: gtk4::Align::Start,
                                    // XXX time to full, fully changed, etc.
                                    #[watch]
                                    set_label: &format!("{} until empty ({:.0}%)", format_duration(model.time_remaining), model.battery_percent),
                                },
                            },
                        },

                        gtk4::Separator {
                        },

                        // Profiles

                        gtk4::Separator {
                        },

                        // Limit charging
                        gtk4::Box {
                            set_orientation: gtk4::Orientation::Horizontal,
                            gtk4::Box {
                                set_orientation: gtk4::Orientation::Vertical,
                                gtk4::Label {
                                    set_halign: gtk4::Align::Start,
                                    set_label: "Limit Battery Charging",
                                },
                                gtk4::Label {
                                    set_halign: gtk4::Align::Start,
                                    set_label: "Increase the lifespan of your battery by setting a maximum charge value of 80%."
                                },
                            },
                            gtk4::Switch {
                                set_valign: gtk4::Align::Center,
                            },
                        },

                        gtk4::Separator {
                        },

                        // Brightness
                        gtk4::Box {
                            #[watch]
                            set_visible: model.backlight.is_some(),
                            set_orientation: gtk4::Orientation::Horizontal,
                            gtk4::Image {
                                set_icon_name: Some("display-brightness-symbolic"),
                            },
                            gtk4::Scale {
                                set_hexpand: true,
                                set_adjustment: &gtk4::Adjustment::new(0., 0., 1., 1., 1., 0.),
                                #[watch]
                                set_value: model.display_brightness,
                                connect_change_value[sender] => move |_, _, value| {
                                    sender.input(AppMsg::SetDisplayBrightness(value));
                                    gtk4::Inhibit(false)
                                },
                            },
                            gtk4::Label {
                                #[watch]
                                set_label: &format!("{:.0}%", model.display_brightness * 100.),
                            },
                        },
                        gtk4::Box {
                            #[watch]
                            set_visible: model.kbd_backlight.is_some(),
                            set_orientation: gtk4::Orientation::Horizontal,
                            gtk4::Image {
                                set_icon_name: Some("keyboard-brightness-symbolic"),
                            },
                            gtk4::Scale {
                                set_hexpand: true,
                                set_adjustment: &gtk4::Adjustment::new(0., 0., 1., 1., 1., 0.),
                                #[watch]
                                set_value: model.keyboard_brightness,
                                connect_change_value[sender] => move |_, _, value| {
                                    sender.input(AppMsg::SetKeyboardBrightness(value));
                                    gtk4::Inhibit(false)
                                },
                            },
                            gtk4::Label {
                                #[watch]
                                set_label: &format!("{:.0}%", model.keyboard_brightness * 100.),
                            },
                        },

                        gtk4::Separator {
                        },

                        gtk4::Button {
                            set_label: "Power Settings...",
                            connect_clicked => move |_| {
                                // XXX open subpanel
                                let _ = Command::new("cosmic-settings").spawn();
                                // TODO hide
                            }
                        }
                    }
                }
            }
        }
    }

    fn init(
        _params: Self::InitParams,
        root: &Self::Root,
        sender: &ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = AppModel {
            icon_name: "battery-symbolic".to_string(),
            ..Default::default()
        };

        let widgets = view_output!();

        match backlight() {
            Ok(Some(backlight)) => {
                if let (Some(brightness), Some(max_brightness)) =
                    (backlight.brightness(), backlight.max_brightness())
                {
                    model.display_brightness = brightness as f64 / max_brightness as f64;
                }
                model.backlight = Some(backlight);
            }
            Ok(None) => {}
            Err(err) => eprintln!("Error finding backlight: {}", err),
        };

        glib::MainContext::default().spawn(glib::clone!(@strong sender => async move {
            match display_device().await {
                Ok(device) => sender.input(AppMsg::SetDevice(device)),
                Err(err) => eprintln!("Failed to open UPower display device: {}", err),
            }
        }));

        glib::MainContext::default().spawn(glib::clone!(@strong sender => async move {
            // XXX avoid multiple connections?
            let proxy = async {
                let connection = zbus::Connection::system().await?;
                LogindSessionProxy::builder(&connection).build().await
            }.await;
            match proxy {
                Ok(session) => sender.input(AppMsg::SetSession(session)),
                Err(err) => eprintln!("Failed to open logind session: {}", err),
            }
        }));

        glib::MainContext::default().spawn(glib::clone!(@strong sender => async move {
            let proxy = async {
                let connection = zbus::Connection::system().await?;
                KbdBacklightProxy::builder(&connection).build().await
            }.await;
            match proxy {
                Ok(kbd_backlight) => sender.input(AppMsg::SetKbdBacklight(kbd_backlight)),
                Err(err) => eprintln!("Failed to open kbd_backlight: {}", err),
            }
        }));

        glib::MainContext::default().spawn(glib::clone!(@strong sender => async move {
            let proxy = async {
                let connection = zbus::Connection::system().await?;
                PowerDaemonProxy::builder(&connection).build().await
            }.await;
            match proxy {
                Ok(power_daemon) => sender.input(AppMsg::SetPowerDaemon(power_daemon)),
                Err(err) => eprintln!("Failed to open power daemon: {}", err),
            }

        }));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: &ComponentSender<Self>) {
        match msg {
            AppMsg::SetDisplayBrightness(value) => {
                self.display_brightness = value;
                // XXX clone
                if let Some(backlight) = self.backlight.clone() {
                    if let Some(session) = self.session.clone() {
                        // XXX cache max brightness
                        if let Some(max_brightness) = backlight.max_brightness() {
                            let value = value.clamp(0., 1.) * (max_brightness as f64);
                            let value = value.round() as u32;
                            // XXX limit queueing?
                            glib::MainContext::default().spawn(async move {
                                if let Err(err) = backlight.set_brightness(&session, value).await {
                                    eprintln!("Failed to set backlight: {}", err);
                                }
                            });
                        }
                    }
                }
            }
            AppMsg::SetKeyboardBrightness(value) => {
                self.keyboard_brightness = value;

                if let Some(kbd_backlight) = self.kbd_backlight.clone() {
                    glib::MainContext::default().spawn(async move {
                        let res = async {
                            // XXX cache
                            let max_brightness = kbd_backlight.get_max_brightness().await?;
                            let value = value.clamp(0., 1.) * (max_brightness as f64);
                            let value = value.round() as i32;
                            kbd_backlight.set_brightness(value).await
                        }
                        .await;
                        if let Err(err) = res {
                            eprintln!("Failed to set keyboard backlight: {}", err);
                        }
                    });
                }
            }
            AppMsg::SetDevice(device) => {
                self.device = Some(device.clone());

                let sender = sender.clone();
                glib::MainContext::default().spawn(async move {
                    let mut stream = futures::stream_select!(
                        device.receive_icon_name_changed().await.map(|_| ()),
                        device.receive_percentage_changed().await.map(|_| ()),
                        device.receive_time_to_empty_changed().await.map(|_| ()),
                    );

                    sender.input(AppMsg::UpdateProperties);
                    while let Some(()) = stream.next().await {
                        sender.input(AppMsg::UpdateProperties);
                    }
                });
            }
            AppMsg::SetSession(session) => {
                self.session = Some(session);
            }
            AppMsg::SetKbdBacklight(kbd_backlight) => {
                self.kbd_backlight = Some(kbd_backlight.clone());

                glib::MainContext::default().spawn(glib::clone!(@strong sender => async move {
                    let res = async {
                        let stream = kbd_backlight.receive_brightness_changed().await?;
                        let brightness = kbd_backlight.get_brightness().await?;
                        let max_brightness = kbd_backlight.get_max_brightness().await?;
                        zbus::Result::Ok((brightness, max_brightness, stream))
                    }.await;
                    match res {
                        Ok((brightness, max_brightness, mut stream)) => {
                            let value = (brightness as f64) / (max_brightness as f64);
                            sender.input(AppMsg::UpdateKbdBrightness(value));
                            while let Some(evt) = stream.next().await {
                                // TODO
                            }
                        }
                        Err(err) => {
                        }
                    }
                }));
            }
            AppMsg::SetPowerDaemon(power_daemon) => {
                self.power_daemon = Some(power_daemon.clone());

                // XXX detect change?
                glib::MainContext::default().spawn(glib::clone!(@strong sender => async move {
                    async {
                        zbus::Result::Ok(if power_daemon.get_switchable().await? {
                            Some(power_daemon.get_graphics().await?)
                        } else {
                            None
                        })
                    };
                }));
                // XXX
            }
            AppMsg::UpdateProperties => {
                if let Some(device) = self.device.as_ref() {
                    if let Ok(Some(percentage)) = device.cached_percentage() {
                        self.battery_percent = percentage;
                    }
                    if let Ok(Some(icon_name)) = device.cached_icon_name() {
                        self.icon_name = icon_name;
                    }
                    if let Ok(Some(secs)) = device.cached_time_to_empty() {
                        self.time_remaining = Duration::from_secs(secs as u64);
                    }
                }
            }
            AppMsg::UpdateKbdBrightness(value) => {
                self.keyboard_brightness = value;
            }
        }
    }
}

fn main() {
    let app: RelmApp<AppModel> = RelmApp::new("com.system76.CosmicAppletBattery");
    app.run(());
}
