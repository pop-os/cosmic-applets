use cosmic::applet::button_theme;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::{
    time,
    widget::{column, row, text, vertical_space},
    window, Alignment, Length, Rectangle, Subscription,
};
use cosmic::iced_core::alignment::{Horizontal, Vertical};
use cosmic::iced_style::application;
use cosmic::theme;
use cosmic::widget::{button, container, divider, grid, Button, Grid, Space};
use cosmic::{app, applet::cosmic_panel_config::PanelAnchor, Command};
use cosmic::{
    widget::{icon, rectangle_tracker::*},
    Element, Theme,
};

use chrono::{DateTime, Datelike, Days, Local, NaiveDate, Timelike};
use std::time::Duration;

use crate::fl;

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Every {
    Minute,
    Second,
}

pub struct Window {
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
pub enum Message {
    TogglePopup,
    CloseRequested(window::Id),
    Tick,
    Rectangle(RectangleUpdate<u32>),
    SelectDay(u32),
    PreviousMonth,
    NextMonth,
}

impl cosmic::Application for Window {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletTime";

    fn init(
        core: app::Core,
        _flags: Self::Flags,
    ) -> (Self, cosmic::iced::Command<app::Message<Self::Message>>) {
        (
            Window {
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
            Message::SelectDay(day) => {
                // TODO
                Command::none()
            }
            Message::PreviousMonth => {
                // TODO
                Command::none()
            }
            Message::NextMonth => {
                // TODO
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
        .on_press(Message::TogglePopup)
        .style(theme::Button::Text);

        if let Some(tracker) = self.rectangle_tracker.as_ref() {
            tracker.container(0, button).into()
        } else {
            button.into()
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let date = text(self.now.format("%B %-d, %Y").to_string()).size(18);
        let day_of_week = text(self.now.format("%A").to_string()).size(14);

        let month_controls = row![
            button::text("<").on_press(Message::PreviousMonth),
            button::text(">").on_press(Message::NextMonth)
        ];

        // Calender
        let monday = get_sunday(self.now.year(), self.now.month());
        let mut day_iter = monday.iter_days();
        let mut calender: Grid<'_, Message> = grid().width(Length::Fill);
        calender = calender.push(
            text("Sun")
                .size(12)
                .width(Length::Fixed(36.0))
                .horizontal_alignment(Horizontal::Center),
        );
        calender = calender.push(
            text("Mon")
                .size(12)
                .width(Length::Fixed(36.0))
                .horizontal_alignment(Horizontal::Center),
        );
        calender = calender.push(
            text("Tue")
                .size(12)
                .width(Length::Fixed(36.0))
                .horizontal_alignment(Horizontal::Center),
        );
        calender = calender.push(
            text("Wed")
                .size(12)
                .width(Length::Fixed(36.0))
                .horizontal_alignment(Horizontal::Center),
        );
        calender = calender.push(
            text("Thu")
                .size(12)
                .width(Length::Fixed(36.0))
                .horizontal_alignment(Horizontal::Center),
        );
        calender = calender.push(
            text("Fri")
                .size(12)
                .width(Length::Fixed(36.0))
                .horizontal_alignment(Horizontal::Center),
        );
        calender = calender.push(
            text("Sat")
                .size(12)
                .width(Length::Fixed(36.0))
                .horizontal_alignment(Horizontal::Center),
        );
        calender = calender.insert_row();

        for i in 0..35 {
            if i > 0 && i % 7 == 0 {
                calender = calender.insert_row();
            }

            let date = day_iter.next().unwrap();
            let day = date.day();
            let month = date.month();
            calender = calender.push(date_button(
                day,
                month == self.now.month(),
                day == self.now.day(),
            ));
        }

        let events = text("No Events this Day")
            .size(12)
            .width(Length::Fill)
            .horizontal_alignment(Horizontal::Center);

        self.core
            .applet
            .popup_container(
                column![
                    row![
                        column![date, day_of_week],
                        Space::with_width(Length::Fill),
                        month_controls,
                    ],
                    calender,
                    container(divider::horizontal::light())
                        .padding([0, 0])
                        .width(Length::Fill),
                    events,
                    container(divider::horizontal::light())
                        .padding([0, 0])
                        .width(Length::Fill),
                    button(text(fl!("datetime-settings")).size(14))
                        .style(button_theme())
                        .padding([8, 24])
                        .width(Length::Fill)
                ]
                .padding([12, 12])
                .spacing(12)
                .align_items(Alignment::Center),
            )
            .into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}

fn date_button(
    day: u32,
    is_month: bool,
    is_day: bool,
) -> Button<'static, Message, cosmic::Renderer> {
    let style = if is_day {
        cosmic::widget::button::Style::Suggested
    } else {
        cosmic::widget::button::Style::Text
    };

    let button = button(
        text(format!("{day}"))
            .size(14.0)
            .horizontal_alignment(Horizontal::Center)
            .vertical_alignment(Vertical::Center),
    )
    .style(style)
    .height(Length::Fixed(36.0))
    .width(Length::Fixed(36.0));

    if is_month {
        button.on_press(Message::SelectDay(day))
    } else {
        button
    }
}

fn get_sunday(year: i32, month: u32) -> NaiveDate {
    let date = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let num_days = date.weekday().num_days_from_sunday();
    date.checked_sub_days(Days::new(num_days as u64)).unwrap()
}
