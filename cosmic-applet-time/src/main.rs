use cosmic::applet::CosmicAppletHelper;
use cosmic::iced::wayland::{
    popup::{destroy_popup, get_popup},
    SurfaceIdWrapper,
};
use cosmic::iced::{
    executor, time,
    widget::{button, column, text},
    window, Alignment, Application, Color, Command, Length, Subscription,
};
use cosmic::iced_style::application::{self, Appearance};
use cosmic::theme;
use cosmic::{Element, Theme};

use chrono::{DateTime, Local, Timelike};
use std::time::Duration;

pub fn main() -> cosmic::iced::Result {
    let mut helper = CosmicAppletHelper::default();
    helper.window_size(120, 16);
    Time::run(helper.window_settings())
}

struct Time {
    applet_helper: CosmicAppletHelper,
    theme: Theme,
    popup: Option<window::Id>,
    id_ctr: u32,
    update_at: Every,
    now: DateTime<Local>,
}

impl Default for Time {
    fn default() -> Self {
        Time {
            applet_helper: CosmicAppletHelper::default(),
            theme: Theme::default(),
            popup: None,
            id_ctr: 0,
            update_at: Every::Minute,
            now: Local::now(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Every {
    Minute,
    Second,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup,
    Tick,
    Ignore,
}

impl Application for Time {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Time, Command<Message>) {
        (Time::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("Time")
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

    fn subscription(&self) -> Subscription<Message> {
        const FALLBACK_DELAY: u64 = 500;
        let update_delay = match self.update_at {
            Every::Minute => chrono::Duration::minutes(1),
            Every::Second => chrono::Duration::seconds(1),
        };

        // Calculate the time until next second/minute so we can sleep the thread until then.
        let now = Local::now().time();
        let next = (now + update_delay)
            .with_second(0)
            .expect("Setting seconds to 0 should always be possible")
            .with_nanosecond(0)
            .expect("Setting nanoseconds to 0 should always be possible.");
        let wait = 1.max((next - now).num_milliseconds());
        time::every(Duration::from_millis(
            wait.try_into().unwrap_or(FALLBACK_DELAY),
        ))
        .map(|_| Message::Tick)
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    self.id_ctr += 1;
                    let new_id = window::Id::new(self.id_ctr);
                    self.popup.replace(new_id);

                    let popup_settings = self.applet_helper.get_popup_settings(
                        window::Id::new(0),
                        new_id,
                        None,
                        Some(60),
                        None,
                    );
                    get_popup(popup_settings)
                }
            }
            Message::Tick => {
                self.now = Local::now();
                Command::none()
            }
            Message::Ignore => Command::none(),
        }
    }

    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => button(
                column![text(self.now.format("%b %-d %-I:%M %p").to_string())]
                    .width(Length::Fill)
                    .align_items(Alignment::Center),
            )
            .on_press(Message::TogglePopup)
            .style(theme::Button::Text)
            .width(Length::Units(120))
            .into(),
            SurfaceIdWrapper::Popup(_) => {
                use std::os::unix::process::ExitStatusExt;
                let calendar = std::str::from_utf8(
                    &std::process::Command::new("happiness")
                        .output()
                        .unwrap_or(std::process::Output {
                            stdout: "`sudo apt install happiness`".as_bytes().to_vec(),
                            stderr: Vec::new(),
                            status: std::process::ExitStatus::from_raw(0),
                        })
                        .stdout,
                )
                .unwrap()
                .to_string();

                let content = column![]
                    .align_items(Alignment::Start)
                    .spacing(12)
                    .padding([24, 0])
                    .push(text(calendar))
                    .padding(8);

                self.applet_helper.popup_container(content).into()
            }
        }
    }
}
