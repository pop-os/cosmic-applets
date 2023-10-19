use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::{
    time,
    widget::{column, text, vertical_space},
    window, Alignment, Length, Rectangle, Subscription,
};
use cosmic::iced_style::application;
use cosmic::{app, applet::cosmic_panel_config::PanelAnchor, Command};
use cosmic::{
    widget::{icon, rectangle_tracker::*},
    Element, Theme,
};

use chrono::{DateTime, Local, Timelike};
use std::time::Duration;

pub fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<Time>(true, ())
}

struct Time {
    core: cosmic::app::Core,
    popup: Option<window::Id>,
    id_ctr: u128,
    update_at: Every,
    now: DateTime<Local>,
    msg: String,
    rectangle_tracker: Option<RectangleTracker<u32>>,
    rectangle: Rectangle,
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
    CloseRequested(window::Id),
    Tick,
    Rectangle(RectangleUpdate<u32>),
}

impl cosmic::Application for Time {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletTime";

    fn init(
        core: app::Core,
        _flags: Self::Flags,
    ) -> (Self, cosmic::iced::Command<app::Message<Self::Message>>) {
        (
            Time {
                core,
                popup: None,
                id_ctr: 0,
                update_at: Every::Minute,
                now: Local::now(),
                msg: String::new(),
                rectangle_tracker: None,
                rectangle: Rectangle::default(),
            },
            Command::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
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
        Subscription::batch(vec![
            rectangle_tracker_subscription(0).map(|e| Message::Rectangle(e.1)),
            time::every(Duration::from_millis(
                wait.try_into().unwrap_or(FALLBACK_DELAY),
            ))
            .map(|_| Message::Tick),
        ])
    }

    fn update(
        &mut self,
        message: Self::Message,
    ) -> cosmic::iced::Command<app::Message<Self::Message>> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
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
                    self.msg = calendar;
                    self.id_ctr += 1;
                    let new_id = window::Id(self.id_ctr);
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        window::Id(0),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    let Rectangle {
                        x,
                        y,
                        width,
                        height,
                    } = self.rectangle;
                    popup_settings.positioner.anchor_rect = Rectangle::<i32> {
                        x: x as i32,
                        y: y as i32,
                        width: width as i32,
                        height: height as i32,
                    };
                    get_popup(popup_settings)
                }
            }
            Message::Tick => {
                self.now = Local::now();
                Command::none()
            }
            Message::Rectangle(u) => {
                match u {
                    RectangleUpdate::Rectangle(r) => {
                        self.rectangle = r.1;
                    }
                    RectangleUpdate::Init(tracker) => {
                        self.rectangle_tracker = Some(tracker);
                    }
                }
                Command::none()
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let button = cosmic::widget::button(
            if matches!(
                self.core.applet.anchor,
                PanelAnchor::Top | PanelAnchor::Bottom
            ) {
                column![
                    cosmic::widget::text(self.now.format("%b %-d %-I:%M %p").to_string()).size(14)
                ]
            } else {
                let mut date_time_col = column![
                    icon::from_name("emoji-recent-symbolic")
                        .size(self.core.applet.suggested_size().0)
                        .symbolic(true),
                    text(self.now.format("%I").to_string()).size(14),
                    text(self.now.format("%M").to_string()).size(14),
                    text(self.now.format("%p").to_string()).size(14),
                    vertical_space(Length::Fixed(4.0)),
                    // TODO better calendar icon?
                    icon::from_name("calendar-go-today-symbolic")
                        .size(self.core.applet.suggested_size().0)
                        .symbolic(true),
                ]
                .align_items(Alignment::Center)
                .spacing(4);
                for d in self.now.format("%x").to_string().split("/") {
                    date_time_col = date_time_col.push(text(d.to_string()).size(14));
                }
                date_time_col
            },
        )
        .style(cosmic::theme::Button::AppletIcon)
        .on_press(Message::TogglePopup);

        if let Some(tracker) = self.rectangle_tracker.as_ref() {
            tracker.container(0, button).into()
        } else {
            button.into()
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let content = column![]
            .align_items(Alignment::Start)
            .spacing(12)
            .padding([24, 0])
            .push(text(&self.msg).size(14))
            .padding(8);

        self.core.applet.popup_container(content).into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}
