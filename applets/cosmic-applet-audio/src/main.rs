use iced::executor;
use iced::widget::{button, column, container, row, svg, text, Column, Slider, Space};
use iced::{Alignment, Application, Command, Element, Length, Settings, Subscription, Theme};

mod pulse;

pub fn main() -> iced::Result {
    Audio::run(Settings {
        window: iced::window::Settings {
            size: (350, 500),
            resizable: true,
            ..iced::window::Settings::default()
        },
        ..Settings::default()
    })
}

#[derive(Default)]
struct Audio {
    sink_vol: f32,
    source_vol: f32,
    is_open: IsOpen,
    outputs: Vec<String>,
    inputs: Vec<String>,
    pulse_state: PulseState,
}

#[derive(Debug, PartialEq, Eq)]
enum IsOpen {
    None,
    Output,
    Input,
}

#[derive(Debug, Clone)]
enum Message {
    SinkChanged(f32),
    SourceChanged(f32),
    OutputToggle,
    InputToggle,
    OutputChanged(String),
    InputChanged(String),
    Send(pulse::Message),
    Pulse(pulse::Event),
}

impl Application for Audio {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Audio, Command<Message>) {
        (
            // TODO unwrap bad. Fix later
            Audio {
                sink_vol: 50.0,
                source_vol: 50.0,
                is_open: IsOpen::None,
                outputs: vec!["1".to_string(), "2".to_string(), "3".to_string()],
                inputs: vec!["1".to_string(), "2".to_string(), "3".to_string()],
                pulse_state: PulseState::Disconnected,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Audio")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::SourceChanged(vol) => {
                self.source_vol = vol;
            }
            Message::SinkChanged(vol) => {
                self.sink_vol = vol;
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
            Message::Send(message) => match &mut self.pulse_state {
                PulseState::Connected(connection) => {}
                PulseState::Disconnected => {} // do nothing
            },
            Message::Pulse(event) => match event {
                pulse::Event::Connected(mut connection) => {
                    connection.send(pulse::Message::GetSinks);
                    connection.send(pulse::Message::GetSources);
                    self.pulse_state = PulseState::Connected(connection);
                }
                pulse::Event::MessageReceived(_) => {}
                pulse::Event::Disconnected => self.pulse_state = PulseState::Disconnected,
            },
        };

        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        pulse::connect().map(Message::Pulse)
    }

    fn view(&self) -> Element<Message> {
        let sink = row![
            icon("status/audio-volume-high-symbolic"),
            Slider::new(1.0..=100.0, self.sink_vol, Message::SinkChanged),
            text(format!("{}%", self.sink_vol.round()))
        ]
        .spacing(10)
        .padding(10);
        let source = row![
            icon("devices/audio-input-microphone-symbolic"),
            Slider::new(1.0..=100.0, self.source_vol, Message::SourceChanged).width(Length::Fill),
            text(format!("{}%", self.source_vol.round()))
        ]
        .spacing(10)
        .padding(10);

        // TODO change these from helper functions to iced components for improved reusability
        let output_drop = revealer(
            self.is_open == IsOpen::Output,
            "Output",
            "Speakers - Built-In Audio",
            self.outputs.clone(),
            Message::OutputToggle,
            Message::OutputChanged(String::from("test")),
        );
        let input_drop = revealer(
            self.is_open == IsOpen::Input,
            "Input",
            "Internal Microphone - Built-In Audio",
            self.inputs.clone(),
            Message::InputToggle,
            Message::InputChanged(String::from("test")),
        );

        let content = Column::new()
            .align_items(Alignment::Start)
            .spacing(20)
            .push(sink)
            .push(source)
            .push(spacer())
            .push(output_drop)
            .push(input_drop);

        container(content)
            .width(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}

fn icon(name: &str) -> iced::widget::Svg {
    svg(svg::Handle::from_path(format!(
        "/usr/share/icons/Pop/scalable/{}.svg",
        name
    )))
    .width(Length::Units(20))
}

// TODO: Make this a themeable widget like the mock-ups
fn spacer() -> iced::widget::Space {
    Space::with_width(Length::Fill)
}

fn revealer<'a>(
    open: bool,
    title: &'a str,
    selected: &'a str,
    options: Vec<String>,
    toggle: Message,
    _change: Message,
) -> iced::widget::Column<'a, Message> {
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
    selected: &'a str,
    toggle: Message,
) -> iced::widget::Button<'a, Message> {
    button(row![row![title].width(Length::Fill), selected])
        .width(Length::Fill)
        .on_press(toggle)
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
