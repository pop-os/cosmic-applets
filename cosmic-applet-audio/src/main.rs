use cosmic::iced::widget;
use cosmic::iced::Limits;
use cosmic::iced_runtime::core::alignment::Horizontal;
use cosmic::theme::Svg;

use cosmic::applet::{applet_button_theme, CosmicAppletHelper};
use cosmic::widget::{button, divider, icon};
use cosmic::Renderer;

use cosmic::iced::{
    self,
    widget::{column, row, slider, text, toggler},
    window, Alignment, Application, Command, Length, Subscription,
};
use cosmic::iced_style::application::{self, Appearance};
use cosmic::{Element, Theme};

use iced::wayland::popup::{destroy_popup, get_popup};
use iced::widget::container;
use iced::Color;

mod pulse;
use crate::pulse::DeviceInfo;
use libpulse_binding::volume::VolumeLinear;

pub fn main() -> cosmic::iced::Result {
    pretty_env_logger::init();

    let helper = CosmicAppletHelper::default();
    Audio::run(helper.window_settings())
}

#[derive(Default)]
struct Audio {
    is_open: IsOpen,
    current_output: Option<DeviceInfo>,
    current_input: Option<DeviceInfo>,
    outputs: Vec<DeviceInfo>,
    inputs: Vec<DeviceInfo>,
    pulse_state: PulseState,
    applet_helper: CosmicAppletHelper,
    icon_name: String,
    theme: Theme,
    popup: Option<window::Id>,
    show_media_controls_in_top_panel: bool,
    id_ctr: u128,
}

#[derive(Debug, PartialEq, Eq)]
enum IsOpen {
    None,
    Output,
    Input,
}

#[derive(Debug, Clone)]
enum Message {
    SetOutputVolume(f64),
    SetInputVolume(f64),
    OutputToggle,
    InputToggle,
    OutputChanged(String),
    InputChanged(String),
    Pulse(pulse::Event),
    Ignore,
    TogglePopup,
    ToggleMediaControlsInTopPanel(bool),
}

impl Application for Audio {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Audio, Command<Message>) {
        (
            Audio {
                is_open: IsOpen::None,
                current_output: None,
                current_input: None,
                outputs: vec![],
                inputs: vec![],
                icon_name: "audio-volume-high-symbolic".to_string(),
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Audio")
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn close_requested(&self, _id: window::Id) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(Box::new(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        }))
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::UpdateConnection);
                    }
                    self.id_ctr += 1;
                    let new_id = window::Id(self.id_ctr);
                    self.popup.replace(new_id);

                    let mut popup_settings = self.applet_helper.get_popup_settings(
                        window::Id(0),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .min_height(1.0)
                        .min_width(1.0)
                        .max_width(400.0)
                        .max_height(1080.0);

                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::GetDefaultSink);
                        conn.send(pulse::Message::GetDefaultSource);
                        conn.send(pulse::Message::GetSinks);
                        conn.send(pulse::Message::GetSources);
                    }

                    return get_popup(popup_settings);
                }
            }
            Message::SetOutputVolume(vol) => {
                self.current_output.as_mut().map(|o| {
                    o.volume
                        .set(o.volume.len(), VolumeLinear(vol / 100.0).into())
                });
                if let PulseState::Connected(connection) = &mut self.pulse_state {
                    if let Some(device) = &self.current_output {
                        if let Some(name) = &device.name {
                            connection.send(pulse::Message::SetSinkVolumeByName(
                                name.clone(),
                                device.volume,
                            ))
                        }
                    }
                }
            }
            Message::SetInputVolume(vol) => {
                self.current_input.as_mut().map(|i| {
                    i.volume
                        .set(i.volume.len(), VolumeLinear(vol / 100.0).into())
                });
                if let PulseState::Connected(connection) = &mut self.pulse_state {
                    if let Some(device) = &self.current_input {
                        if let Some(name) = &device.name {
                            log::info!("increasing volume of {}", name);
                            connection.send(pulse::Message::SetSourceVolumeByName(
                                name.clone(),
                                device.volume,
                            ))
                        }
                    }
                }
            }
            Message::OutputChanged(val) => {
                if let Some(conn) = self.pulse_state.connection() {
                    if let Some(val) = self.outputs.iter().find(|o| o.name.as_ref() == Some(&val)) {
                        conn.send(pulse::Message::SetDefaultSink(val.clone()));
                    }
                }
            }
            Message::InputChanged(val) => {
                if let Some(conn) = self.pulse_state.connection() {
                    if let Some(val) = self.inputs.iter().find(|i| i.name.as_ref() == Some(&val)) {
                        conn.send(pulse::Message::SetDefaultSource(val.clone()));
                    }
                }
            }
            Message::OutputToggle => {
                self.is_open = if self.is_open == IsOpen::Output {
                    IsOpen::None
                } else {
                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::GetSinks);
                    }
                    IsOpen::Output
                }
            }
            Message::InputToggle => {
                self.is_open = if self.is_open == IsOpen::Input {
                    IsOpen::None
                } else {
                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::GetSources);
                    }
                    IsOpen::Input
                }
            }
            Message::Pulse(event) => match event {
                pulse::Event::Init(conn) => self.pulse_state = PulseState::Disconnected(conn),
                pulse::Event::Connected => {
                    self.pulse_state.connected();

                    if let Some(conn) = self.pulse_state.connection() {
                        conn.send(pulse::Message::GetSinks);
                        conn.send(pulse::Message::GetSources);
                        conn.send(pulse::Message::GetDefaultSink);
                        conn.send(pulse::Message::GetDefaultSource);
                    }
                }
                pulse::Event::MessageReceived(msg) => {
                    match msg {
                        // This is where we match messages from the subscription to app state
                        pulse::Message::SetSinks(sinks) => self.outputs = sinks,
                        pulse::Message::SetSources(sources) => {
                            self.inputs = sources
                                .into_iter()
                                .filter(|source| {
                                    !source
                                        .name
                                        .as_ref()
                                        .unwrap_or(&String::from("Generic"))
                                        .contains("monitor")
                                })
                                .collect()
                        }
                        pulse::Message::SetDefaultSink(sink) => {
                            self.current_output = Some(sink);
                        }
                        pulse::Message::SetDefaultSource(source) => {
                            self.current_input = Some(source)
                        }
                        pulse::Message::Disconnected => {
                            panic!("Subscriton error handling is bad. This should never happen.")
                        }
                        _ => {
                            log::trace!("Received misc message")
                        }
                    }
                }
                pulse::Event::Disconnected => self.pulse_state.disconnected(),
            },
            Message::Ignore => {}
            Message::ToggleMediaControlsInTopPanel(enabled) => {
                self.show_media_controls_in_top_panel = enabled;
            }
        };

        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        pulse::connect().map(Message::Pulse)
    }

    fn view(&self, id: window::Id) -> Element<Message> {
        if id == window::Id(0) {
            self.applet_helper
                .icon_button(&self.icon_name)
                .on_press(Message::TogglePopup)
                .into()
        } else {
            let audio_disabled = matches!(self.pulse_state, PulseState::Disconnected(_));
            let out_f64 = VolumeLinear::from(
                self.current_output
                    .as_ref()
                    .map(|o| o.volume.avg())
                    .unwrap_or_default(),
            )
            .0 * 100.0;
            let in_f64 = VolumeLinear::from(
                self.current_input
                    .as_ref()
                    .map(|o| o.volume.avg())
                    .unwrap_or_default(),
            )
            .0 * 100.0;

            let audio_content = if audio_disabled {
                column![text("PulseAudio Disconnected")
                    .width(Length::Fill)
                    .horizontal_alignment(Horizontal::Center)
                    .size(24),]
            } else {
                column![
                    row![
                        icon("audio-volume-high-symbolic", 32)
                            .width(Length::Fixed(24.0))
                            .height(Length::Fixed(24.0))
                            .style(Svg::Symbolic),
                        slider(0.0..=100.0, out_f64, Message::SetOutputVolume)
                            .width(Length::FillPortion(5)),
                        text(format!("{}%", out_f64.round()))
                            .size(16)
                            .width(Length::FillPortion(1))
                            .horizontal_alignment(Horizontal::Right)
                    ]
                    .spacing(12)
                    .align_items(Alignment::Center)
                    .padding([8, 24]),
                    row![
                        icon("audio-input-microphone-symbolic", 32)
                            .width(Length::Fixed(24.0))
                            .height(Length::Fixed(24.0))
                            .style(Svg::Symbolic),
                        slider(0.0..=100.0, in_f64, Message::SetInputVolume)
                            .width(Length::FillPortion(5)),
                        text(format!("{}%", in_f64.round()))
                            .size(16)
                            .width(Length::FillPortion(1))
                            .horizontal_alignment(Horizontal::Right)
                    ]
                    .spacing(12)
                    .align_items(Alignment::Center)
                    .padding([8, 24]),
                    container(divider::horizontal::light())
                        .padding([12, 24])
                        .width(Length::Fill),
                    revealer(
                        self.is_open == IsOpen::Output,
                        "Output",
                        match &self.current_output {
                            Some(output) => pretty_name(output.description.clone()),
                            None => String::from("No device selected"),
                        },
                        self.outputs
                            .clone()
                            .into_iter()
                            .map(|output| (
                                output.name.clone().unwrap_or_default(),
                                pretty_name(output.description)
                            ))
                            .collect(),
                        Message::OutputToggle,
                        Message::OutputChanged,
                    ),
                    revealer(
                        self.is_open == IsOpen::Input,
                        "Input",
                        match &self.current_input {
                            Some(input) => pretty_name(input.description.clone()),
                            None => String::from("No device selected"),
                        },
                        self.inputs
                            .clone()
                            .into_iter()
                            .map(|input| (
                                input.name.clone().unwrap_or_default(),
                                pretty_name(input.description)
                            ))
                            .collect(),
                        Message::InputToggle,
                        Message::InputChanged,
                    )
                ]
                .align_items(Alignment::Start)
            };
            let content = column![
                audio_content,
                container(divider::horizontal::light())
                    .padding([12, 24])
                    .width(Length::Fill),
                container(
                    toggler(
                        Some("Show Media Controls on Top Panel".into()),
                        self.show_media_controls_in_top_panel,
                        Message::ToggleMediaControlsInTopPanel,
                    )
                    .text_size(14)
                )
                .padding([0, 24]),
                container(divider::horizontal::light())
                    .padding([12, 24])
                    .width(Length::Fill),
                button(applet_button_theme())
                    .custom(vec![text("Sound Settings...").size(14).into()])
                    .padding([8, 24])
                    .width(Length::Fill)
            ]
            .align_items(Alignment::Start)
            .padding([8, 0]);

            self.applet_helper
                .popup_container(container(content))
                .into()
        }
    }
}

fn revealer(
    open: bool,
    title: &str,
    selected: String,
    options: Vec<(String, String)>,
    toggle: Message,
    mut change: impl FnMut(String) -> Message + 'static,
) -> widget::Column<Message, Renderer> {
    if open {
        options.iter().fold(
            column![revealer_head(open, title, selected, toggle)].width(Length::Fill),
            |col, (id, name)| {
                col.push(
                    button(applet_button_theme())
                        .custom(vec![text(name).size(14).into()])
                        .on_press(change(id.clone()))
                        .width(Length::Fill)
                        .padding([8, 48]),
                )
            },
        )
    } else {
        column![revealer_head(open, title, selected, toggle)]
    }
}

fn revealer_head(
    _open: bool,
    title: &str,
    selected: String,
    toggle: Message,
) -> widget::Button<Message, Renderer> {
    button(applet_button_theme())
        .custom(vec![
            text(title).width(Length::Fill).size(14).into(),
            text(selected).size(10).into(),
        ])
        .padding([8, 24])
        .width(Length::Fill)
        .on_press(toggle)
}

fn pretty_name(name: Option<String>) -> String {
    match name {
        Some(n) => n,
        None => String::from("Generic"),
    }
}

#[derive(Default)]
enum PulseState {
    #[default]
    Init,
    Disconnected(pulse::Connection),
    Connected(pulse::Connection),
}

impl PulseState {
    fn connection(&mut self) -> Option<&mut pulse::Connection> {
        match self {
            PulseState::Disconnected(c) => Some(c),
            PulseState::Connected(c) => Some(c),
            PulseState::Init => None,
        }
    }

    fn connected(&mut self) {
        if let PulseState::Disconnected(c) = self {
            *self = PulseState::Connected(c.clone());
        }
    }

    fn disconnected(&mut self) {
        if let PulseState::Connected(c) = self {
            *self = PulseState::Disconnected(c.clone());
        }
    }
}

impl Default for IsOpen {
    fn default() -> Self {
        IsOpen::None
    }
}
