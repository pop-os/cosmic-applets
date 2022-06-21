use futures::prelude::*;
use gtk4::{glib, prelude::*};
use relm4::{ComponentParts, ComponentSender, RelmApp, SimpleComponent, WidgetPlus};
use std::time::Duration;

mod backlight;
mod upower;
use upower::UPowerProxy;
mod upower_device;
use upower_device::DeviceProxy;

async fn display_device() -> zbus::Result<DeviceProxy<'static>> {
    let connection = zbus::Connection::system().await?;
    let upower = UPowerProxy::new(&connection).await?;
    let device_path = upower.get_display_device().await?;
    DeviceProxy::builder(&connection)
        .path(device_path)?
        .build()
        .await
}

async fn foo(device: &DeviceProxy<'static>) {
    let mut icon_name_stream = device.receive_icon_name_changed().await;
    let mut battery_level_stream = device.receive_battery_level_changed().await;

    glib::MainContext::default()
        .spawn(async move { while let Some(evt) = icon_name_stream.next().await {} });

    glib::MainContext::default()
        .spawn(async move { while let Some(evt) = battery_level_stream.next().await {} });
}

#[derive(Default)]
struct AppModel {
    icon_name: String,
    battery_percent: u8,
    time_remaining: Duration,
    display_brightness: f64,
    keyboard_brightness: f64,
}

enum AppMsg {
    SetDisplayBrightness(f64),
    SetKeyboardBrightness(f64),
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
                                    // XXX duration formatting
                                    #[watch]
                                    set_label: &format!("{:?} until empty ({}%)", model.time_remaining, model.battery_percent),
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
                            set_orientation: gtk4::Orientation::Horizontal,
                            gtk4::Image {
                                set_icon_name: Some("display-brightness-symbolic"),
                            },
                            gtk4::Scale {
                                set_hexpand: true,
                                set_adjustment: &gtk4::Adjustment::new(0., 0., 100., 1., 1., 0.),
                                #[watch]
                                set_value: model.display_brightness,
                                connect_change_value[sender] => move |_, _, value| {
                                    sender.input(AppMsg::SetDisplayBrightness(value));
                                    gtk4::Inhibit(false)
                                },
                            },
                            gtk4::Label {
                                #[watch]
                                set_label: &format!("{:.0}%", model.display_brightness),
                            },
                        },
                        gtk4::Box {
                            set_orientation: gtk4::Orientation::Horizontal,
                            gtk4::Image {
                                set_icon_name: Some("keyboard-brightness-symbolic"),
                            },
                            gtk4::Scale {
                                set_hexpand: true,
                                set_adjustment: &gtk4::Adjustment::new(0., 0., 100., 1., 1., 0.),
                                #[watch]
                                set_value: model.keyboard_brightness,
                                connect_change_value[sender] => move |_, _, value| {
                                    sender.input(AppMsg::SetKeyboardBrightness(value));
                                    gtk4::Inhibit(false)
                                },
                            },
                            gtk4::Label {
                                #[watch]
                                set_label: &format!("{:.0}%", model.keyboard_brightness),
                            },
                        },

                        gtk4::Separator {
                        },

                        gtk4::Button {
                            set_label: "Power Settings...",
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
        let model = AppModel {
            icon_name: "battery-symbolic".to_string(),
            ..Default::default()
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: &ComponentSender<Self>) {
        match msg {
            AppMsg::SetDisplayBrightness(value) => {
                self.display_brightness = value;
            }
            AppMsg::SetKeyboardBrightness(value) => {
                self.keyboard_brightness = value;
            }
        }
    }
}

fn main() {
    let app: RelmApp<AppModel> = RelmApp::new("com.system76.CosmicAppletBattery");
    app.run(());
}
