use cosmic::{
    applet::CosmicAppletHelper,
    iced::{
        executor,
        widget::{column, container, row, text},
        Alignment, Application, Color, Command, Length, Subscription,
    },
    iced_native::window,
    iced_style::{application, svg},
    theme::Svg,
    widget::{horizontal_rule, icon, toggler},
    Element, Theme,
};
use futures::{channel::mpsc::UnboundedSender, SinkExt};
use iced_sctk::{
    application::SurfaceIdWrapper,
    commands::popup::{destroy_popup, get_popup},
};

use crate::{
    config, fl,
    network_manager::{
        available_wifi::AccessPoint, current_networks::ActiveConnectionInfo,
        network_manager_subscription, NetworkManagerEvent, NetworkManagerRequest,
    },
};

pub fn run() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    CosmicNetworkApplet::run(helper.window_settings())
}

#[derive(Clone, Default)]
struct CosmicNetworkApplet {
    icon_name: String,
    theme: Theme,
    popup: Option<window::Id>,
    id_ctr: u32,
    applet_helper: CosmicAppletHelper,
    // STATE
    airplane_mode: bool,
    wifi: bool,
    wireless_access_points: Vec<AccessPoint>,
    active_conns: Vec<ActiveConnectionInfo>,
    nm_sender: Option<UnboundedSender<NetworkManagerRequest>>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    ToggleAirplaneMode(bool),
    ToggleWiFi(bool),
    Errored(String),
    Ignore,
    NetworkManagerEvent(NetworkManagerEvent),
}

impl Application for CosmicNetworkApplet {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            CosmicNetworkApplet {
                icon_name: "network-offline-symbolic".to_string(),
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
                        (400, 240),
                        None,
                        None,
                    );
                    popup_settings.positioner.offset.0 = 200;
                    return get_popup(popup_settings);
                }
            }
            Message::Errored(_) => todo!(),
            Message::Ignore => {}
            Message::ToggleAirplaneMode(enabled) => {
                self.airplane_mode = enabled;
                // TODO apply changes
            }
            Message::ToggleWiFi(enabled) => {
                self.wifi = enabled;
                if let Some(tx) = self.nm_sender.as_mut() {
                    let _ = tx.unbounded_send(NetworkManagerRequest::SetWiFi(enabled));
                }
            }
            Message::NetworkManagerEvent(event) => match event {
                NetworkManagerEvent::Init {
                    sender,
                    wireless_access_points,
                    active_conns,
                    wifi_enabled,
                    airplane_mode,
                } => {
                    self.nm_sender.replace(sender);
                    self.wireless_access_points = wireless_access_points;
                    self.active_conns = active_conns;
                    self.wifi = wifi_enabled;
                    self.airplane_mode = airplane_mode;
                }
                NetworkManagerEvent::WiFiEnabled(enabled) => {
                    self.wifi = enabled;
                }
                NetworkManagerEvent::WirelessAccessPoints(access_points) => {
                    self.wireless_access_points = access_points;
                }
                NetworkManagerEvent::ActiveConns(conns) => {
                    self.active_conns = conns;
                }
            },
        }
        Command::none()
    }
    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => self
                .applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into(),
            SurfaceIdWrapper::Popup(_) => {
                let name = text(fl!("network")).size(18);
                let icon = icon(&self.icon_name, 24)
                    .style(Svg::Custom(|theme| svg::Appearance {
                        fill: Some(theme.palette().text),
                    }))
                    .width(Length::Units(24))
                    .height(Length::Units(24));
                self.applet_helper
                    .popup_container(
                        column![
                            row![icon, name].spacing(8).width(Length::Fill),
                            column![] // TODO active connections
                                .padding([8, 0])
                                .width(Length::Fill),
                            horizontal_rule(1),
                            container(
                                toggler(fl!("airplane-mode"), self.airplane_mode, |m| {
                                    Message::ToggleAirplaneMode(m)
                                })
                                .width(Length::Fill)
                            )
                            .padding([0, 12]),
                            horizontal_rule(1),
                            container(
                                toggler(fl!("wifi"), self.wifi, |m| { Message::ToggleWiFi(m) })
                                    .width(Length::Fill)
                            )
                            .padding([0, 12]),
                            column![] // TODO wifi list
                        ]
                        .align_items(Alignment::Center)
                        .spacing(8)
                        .padding(8),
                    )
                    .into()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        network_manager_subscription(0).map(|(_, event)| Message::NetworkManagerEvent(event))
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn close_requested(&self, _id: iced_sctk::application::SurfaceIdWrapper) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| application::Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }
}
