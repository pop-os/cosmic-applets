use std::f32::consts::E;

use crate::bluetooth::{BluerDeviceStatus, BluerRequest, BluerState};
use cosmic::applet::APPLET_BUTTON_THEME;
use cosmic::iced_style;
use cosmic::widget::ListColumn;
use cosmic::{
    applet::CosmicAppletHelper,
    iced::{
        wayland::{
            popup::{destroy_popup, get_popup},
            SurfaceIdWrapper,
        },
        widget::{column, container, row, scrollable, text, text_input, Column},
        Alignment, Application, Color, Command, Length, Subscription,
    },
    iced_native::{
        alignment::{Horizontal, Vertical},
        layout::Limits,
        renderer::BorderRadius,
        window,
    },
    iced_style::{application, button::StyleSheet, svg},
    theme::{Button, Svg},
    widget::{button, divider, icon, toggler},
    Element, Theme,
};
use tokio::sync::mpsc::Sender;

use crate::bluetooth::{bluetooth_subscription, BluerEvent};
use crate::{config, fl};

pub fn run() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    CosmicBluetoothApplet::run(helper.window_settings())
}

#[derive(Debug)]
enum NewConnectionState {
    EnterPassword { device: (), password: String },
    Waiting(()),
    Failure(()),
}

// impl Into<()> for NewConnectionState {
//     fn into(self) -> AccessPoint {
//         match self {
//             NewConnectionState::EnterPassword {
//                 access_point,
//                 password,
//             } => access_point,
//             NewConnectionState::Waiting(access_point) => access_point,
//             NewConnectionState::Failure(access_point) => access_point,
//         }
//     }
// }

#[derive(Default)]
struct CosmicBluetoothApplet {
    icon_name: String,
    theme: Theme,
    popup: Option<window::Id>,
    id_ctr: u32,
    applet_helper: CosmicAppletHelper,
    bluer_state: BluerState,
    bluer_sender: Option<Sender<BluerRequest>>,
    // UI state
    show_visible_devices: bool,
    new_connection: Option<NewConnectionState>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    ToggleVisibleDevices(bool),
    Errored(String),
    Ignore,
    BluetoothEvent(BluerEvent),
    Request(BluerRequest),
}

impl Application for CosmicBluetoothApplet {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            CosmicBluetoothApplet {
                icon_name: "bluetooth-symbolic".to_string(),
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        config::APP_ID.to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    // TODO request update of state maybe
                    self.id_ctr += 1;
                    let new_id = window::Id::new(self.id_ctr);
                    self.popup.replace(new_id);

                    let mut popup_settings = self.applet_helper.get_popup_settings(
                        window::Id::new(0),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    popup_settings.positioner.size_limits = Limits::NONE
                        .min_height(1)
                        .min_width(1)
                        .max_height(800)
                        .max_width(400);
                    return get_popup(popup_settings);
                }
            }
            Message::Errored(_) => todo!(),
            Message::Ignore => {}
            // Message::SelectDevice(device) => {
            // let tx = if let Some(tx) = self.nm_sender.as_ref() {
            //     tx
            // } else {
            //     return Command::none();
            // };

            // let _ = tx.unbounded_send(NetworkManagerRequest::SelectAccessPoint(
            //     access_point.ssid.clone(),
            // ));

            // self.new_connection
            //     .replace(NewConnectionState::EnterPassword {
            //         access_point,
            //         password: String::new(),
            //     });
            // }
            Message::ToggleVisibleDevices(enabled) => {
                self.new_connection.take();
                self.show_visible_devices = enabled;
            }
            Message::BluetoothEvent(e) => match e {
                BluerEvent::RequestResponse {
                    req: _req,
                    state,
                    err_msg,
                } => {
                    if let Some(err_msg) = err_msg {
                        eprintln!("bluetooth request error: {}", err_msg);
                    }
                    dbg!(&state);
                    self.bluer_state = state;
                    // TODO special handling for some requests
                }
                BluerEvent::Init { sender, state } => {
                    self.bluer_sender.replace(sender);
                    self.bluer_state = state;
                }
                BluerEvent::DevicesChanged { state } => {
                    self.bluer_state = state;
                }
                BluerEvent::Finished => {
                    // TODO exit?
                    todo!()
                }
            },
            Message::Request(r) => {
                match &r {
                    BluerRequest::SetBluetoothEnabled(enabled) => {
                        self.bluer_state.bluetooth_enabled = *enabled;
                        if !*enabled {
                            self.bluer_state = BluerState::default();
                        }
                    }
                    BluerRequest::ConnectDevice(add) => {
                        self.bluer_state
                            .devices
                            .iter_mut()
                            .find(|d| d.address == *add)
                            .map(|d| {
                                d.status = BluerDeviceStatus::Connecting;
                            });
                    }
                    BluerRequest::DisconnectDevice(add) => {
                        self.bluer_state
                            .devices
                            .iter_mut()
                            .find(|d| d.address == *add)
                            .map(|d| {
                                d.status = BluerDeviceStatus::Disconnecting;
                            });
                    }
                    BluerRequest::PairDevice(add) => {
                        self.bluer_state
                            .devices
                            .iter_mut()
                            .find(|d| d.address == *add)
                            .map(|d| {
                                d.status = BluerDeviceStatus::Pairing;
                            });
                    }
                    _ => {} // TODO
                }
                if let Some(tx) = self.bluer_sender.as_mut().cloned() {
                    return Command::perform(
                        async move {
                            let _ = tx.send(r).await;
                        },
                        |_| Message::Ignore, // Error handling
                    );
                }
            }
        }
        Command::none()
    }
    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        let button_style = Button::Custom {
            active: |t| iced_style::button::Appearance {
                border_radius: BorderRadius::from(0.0),
                ..t.active(&Button::Text)
            },
            hover: |t| iced_style::button::Appearance {
                border_radius: BorderRadius::from(0.0),
                ..t.hovered(&Button::Text)
            },
        };
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => self
                .applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into(),
            SurfaceIdWrapper::Popup(_) => {
                let mut known_bluetooth = column![];
                for dev in &self.bluer_state.devices {
                    let mut row = row![].align_items(Alignment::Center);
                    row = row.push(
                        text(dev.name.clone())
                            .horizontal_alignment(Horizontal::Left)
                            .vertical_alignment(Vertical::Center)
                            .width(Length::Fill),
                    );
                    match &dev.status {
                        BluerDeviceStatus::Connected => {
                            row = row.push(
                                text(fl!("connected"))
                                    .horizontal_alignment(Horizontal::Right)
                                    .vertical_alignment(Vertical::Center),
                            );
                        }
                        BluerDeviceStatus::Paired => {}
                        BluerDeviceStatus::Connecting | BluerDeviceStatus::Disconnecting => {
                            row = row.push(
                                icon("process-working-symbolic", 24)
                                    .style(Svg::Custom(|theme| svg::Appearance {
                                        color: Some(theme.palette().text),
                                    }))
                                    .width(Length::Units(24))
                                    .height(Length::Units(24)),
                            );
                        }
                        BluerDeviceStatus::Disconnected | BluerDeviceStatus::Pairing => continue,
                    };

                    known_bluetooth = known_bluetooth.push(
                        button(APPLET_BUTTON_THEME)
                            .custom(vec![row.into()])
                            .on_press(match dev.status {
                                BluerDeviceStatus::Connected => {
                                    Message::Request(BluerRequest::DisconnectDevice(dev.address))
                                }
                                BluerDeviceStatus::Disconnected => {
                                    Message::Request(BluerRequest::PairDevice(dev.address))
                                }
                                BluerDeviceStatus::Paired => {
                                    Message::Request(BluerRequest::ConnectDevice(dev.address))
                                }
                                BluerDeviceStatus::Connecting => {
                                    Message::Request(BluerRequest::CancelConnect(dev.address))
                                }
                                BluerDeviceStatus::Disconnecting => Message::Ignore, // Start connecting?
                                BluerDeviceStatus::Pairing => Message::Ignore, // Cancel pairing?
                            })
                            .width(Length::Fill),
                    );
                }

                let mut content = column![
                    container(
                        toggler(fl!("bluetooth"), self.bluer_state.bluetooth_enabled, |m| {
                            Message::Request(BluerRequest::SetBluetoothEnabled(m))
                        },)
                        .width(Length::Fill)
                    )
                    .padding([0, 12]),
                    divider::horizontal::light(),
                    known_bluetooth,
                ]
                .align_items(Alignment::Center)
                .spacing(8)
                .padding([8, 0]);
                let dropdown_icon = if self.show_visible_devices {
                    "go-down-symbolic"
                } else {
                    "go-next-symbolic"
                };
                let available_connections_btn = button(Button::Secondary)
                    .custom(
                        vec![
                            text(fl!("other-devices"))
                                .size(14)
                                .width(Length::Fill)
                                .height(Length::Units(24))
                                .vertical_alignment(Vertical::Center)
                                .into(),
                            container(
                                icon(dropdown_icon, 14)
                                    .style(Svg::Custom(|theme| svg::Appearance {
                                        color: Some(theme.palette().text),
                                    }))
                                    .width(Length::Units(14))
                                    .height(Length::Units(14)),
                            )
                            .align_x(Horizontal::Center)
                            .align_y(Vertical::Center)
                            .width(Length::Units(24))
                            .height(Length::Units(24))
                            .into(),
                        ]
                        .into(),
                    )
                    .padding([8, 24])
                    .style(button_style.clone())
                    .on_press(Message::ToggleVisibleDevices(!self.show_visible_devices));
                content = content.push(available_connections_btn);
                if self.show_visible_devices {
                    let mut list_column = Vec::with_capacity(self.bluer_state.devices.len());

                    if self.bluer_state.bluetooth_enabled {
                        let mut visible_devices = column![];
                        for dev in self.bluer_state.devices.iter().filter(|d| {
                            matches!(
                                d.status,
                                BluerDeviceStatus::Disconnected | BluerDeviceStatus::Pairing
                            )
                        }) {
                            let mut row = row![].width(Length::Fill).align_items(Alignment::Center);
                            row = row.push(
                                text(dev.name.clone()).horizontal_alignment(Horizontal::Left),
                            );
                            visible_devices = visible_devices.push(
                                button(APPLET_BUTTON_THEME)
                                    .custom(vec![row.width(Length::Fill).into()])
                                    .on_press(Message::Request(BluerRequest::PairDevice(
                                        dev.address.clone(),
                                    )))
                                    .width(Length::Fill),
                            );
                        }
                        list_column.push(visible_devices.into());
                    }
                    let num_dev = list_column.len();
                    if num_dev > 5 {
                        content = content.push(
                            scrollable(Column::with_children(list_column))
                                .height(Length::Units(300)),
                        );
                    } else {
                        content = content.push(Column::with_children(list_column));
                    }
                }
                self.applet_helper.popup_container(content).into()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        bluetooth_subscription(0).map(|e| Message::BluetoothEvent(e.1))
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn close_requested(&self, _id: SurfaceIdWrapper) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| application::Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }
}
