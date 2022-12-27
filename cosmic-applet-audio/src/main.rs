use cosmic::iced::wayland::SurfaceIdWrapper;
use cosmic::iced::widget;
use cosmic::iced_native::alignment::Horizontal;
use cosmic::theme::Svg;
use iced::widget::Space;

use cosmic::applet::CosmicAppletHelper;
use cosmic::widget::icon;
use cosmic::Renderer;

use cosmic::iced::{
    self, executor,
    widget::{button, column, row, slider, text},
    window, Alignment, Application, Command, Length, Subscription,
};
use cosmic::iced_style::application::{self, Appearance};
use cosmic::{Element, Theme};

use iced::wayland::popup::{destroy_popup, get_popup};
use iced::widget::container;
use iced::Color;

mod pulse;
use crate::pulse::DeviceInfo;
use libpulse_binding::volume::{Volume, VolumeLinear};

pub fn main() -> cosmic::iced::Result {
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
    id_ctr: u32,
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
}

impl Application for Audio {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Audio, Command<Message>) {
        (
            Audio {
                is_open: IsOpen::None,
                current_output: None,
                current_input: None,
                outputs: vec![],
                inputs: vec![],
                pulse_state: PulseState::Disconnected,
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

    fn close_requested(&self, _id: SurfaceIdWrapper) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    self.id_ctr += 1;
                    let new_id = window::Id::new(self.id_ctr);
                    self.popup.replace(new_id);

                    let popup_settings = self.applet_helper.get_popup_settings(
                        window::Id::new(0),
                        new_id,
                        Some((400, 300)),
                        None,
                        None,
                    );
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
                                name.clone().to_string(),
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
                            println!("increasing volume of {}", name);
                            connection.send(pulse::Message::SetSourceVolumeByName(
                                name.clone().to_string(),
                                device.volume,
                            ))
                        }
                    }
                }
            }
            Message::OutputChanged(val) => println!("changed output {}", val),
            Message::InputChanged(val) => println!("changed input {}", val),
            Message::OutputToggle => {
                self.is_open = if self.is_open == IsOpen::Output {
                    IsOpen::None
                } else {
                    IsOpen::Output
                }
            }
            Message::InputToggle => {
                self.is_open = if self.is_open == IsOpen::Input {
                    IsOpen::None
                } else {
                    IsOpen::Input
                }
            }
            Message::Pulse(event) => match event {
                pulse::Event::Connected(mut connection) => {
                    connection.send(pulse::Message::GetSinks);
                    connection.send(pulse::Message::GetSources);
                    connection.send(pulse::Message::GetDefaultSink);
                    connection.send(pulse::Message::GetDefaultSource);
                    self.pulse_state = PulseState::Connected(connection);
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
                            println!("Received misc message")
                        }
                    }
                }
                // TODO: view() should gray out buttons/slider when state is disconnected
                pulse::Event::Disconnected => {
                    println!("setting state to disconnected");
                    self.pulse_state = PulseState::Disconnected
                }
            },
            Message::Ignore => {}
        };

        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        pulse::connect().map(Message::Pulse)
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
                let out_f64 = VolumeLinear::from(
                    self.current_output
                        .as_ref()
                        .map(|o| o.volume.avg())
                        .unwrap_or(Volume::default()),
                )
                .0 * 100.0;
                let in_f64 = VolumeLinear::from(
                    self.current_input
                        .as_ref()
                        .map(|o| o.volume.avg())
                        .unwrap_or(Volume::default()),
                )
                .0 * 100.0;

                let sink = row![
                    icon("audio-volume-high-symbolic", 64).width(Length::Units(24)).height(Length::Units(24)).style(Svg::SymbolicActive),
                    slider(0.0..=100.0, out_f64, Message::SetOutputVolume).width(Length::FillPortion(5)),
                    text(format!("{}%", out_f64.round())).width(Length::FillPortion(1)).horizontal_alignment(Horizontal::Right)
                ]
                .spacing(10)
                .align_items(Alignment::Center);
                let source = row![
                    icon("audio-input-microphone-symbolic", 64).width(Length::Units(24)).height(Length::Units(24)).style(Svg::SymbolicActive),
                    slider(0.0..=100.0, in_f64, Message::SetInputVolume).width(Length::FillPortion(5)),
                    text(format!("{}%", in_f64.round())).width(Length::FillPortion(1)).horizontal_alignment(Horizontal::Right)
                ]
                .spacing(10)
                .align_items(Alignment::Center);

                // TODO change these from helper functions to iced components for improved reusability
                let output_drop = revealer(
                    self.is_open == IsOpen::Output,
                    "Output",
                    match &self.current_output {
                        Some(output) => pretty_name(output.description.clone()),
                        None => String::from("No device selected"),
                    },
                    self.outputs
                        .clone()
                        .into_iter()
                        .map(|output| pretty_name(output.description))
                        .collect(),
                    Message::OutputToggle,
                    Message::OutputChanged(String::from("test")),
                );
                let input_drop = revealer(
                    self.is_open == IsOpen::Input,
                    "Input",
                    match &self.current_input {
                        Some(input) => pretty_name(input.description.clone()),
                        None => String::from("No device selected"),
                    },
                    self.inputs
                        .clone()
                        .into_iter()
                        .map(|input| pretty_name(input.description))
                        .collect(),
                    Message::InputToggle,
                    Message::InputChanged(String::from("test")),
                );

                let content = column![]
                    .align_items(Alignment::Start)
                    .spacing(20)
                    .push(sink)
                    .push(source)
                    .push(spacer())
                    .push(output_drop)
                    .push(input_drop)
                    .padding(8);

                self.applet_helper
                    .popup_container(container(content))
                    .into()
            }
        }
    }
}

// TODO: Make this a themeable widget like the mock-ups
fn spacer() -> iced::widget::Space {
    Space::with_width(Length::Fill)
}

fn revealer<'a>(
    open: bool,
    title: &'a str,
    selected: String,
    options: Vec<String>,
    toggle: Message,
    _change: Message,
) -> widget::Column<'a, Message, Renderer> {
    if open {
        options.iter().fold(
            column![revealer_head(open, title, selected, toggle)].width(Length::Fill),
            |col, device| col.push(text(device)),
        )
    } else {
        column![revealer_head(open, title, selected, toggle)]
    }
}

fn revealer_head<'a>(
    _open: bool,
    title: &'a str,
    selected: String,
    toggle: Message,
) -> widget::Button<Message, Renderer> {
    button(row![row![title].width(Length::Fill), text(selected)])
        .width(Length::Fill)
        .on_press(toggle)
}

fn pretty_name(name: Option<String>) -> String {
    match name {
        Some(n) => n,
        None => String::from("Generic"),
    }
}

enum PulseState {
    Disconnected,
    Connected(pulse::Connection),
}

impl Default for PulseState {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl Default for IsOpen {
    fn default() -> Self {
        IsOpen::None
    }
}
