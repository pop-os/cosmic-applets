use cosmic::applet::{menu_button, padded_control};
use cosmic::cctk::sctk::reexports::calloop;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::{
    time,
    widget::{column, row, text, vertical_space},
    window, Alignment, Length, Rectangle, Subscription,
};
use cosmic::iced_core::alignment::{Horizontal, Vertical};
use cosmic::iced_style::application;
use cosmic::widget::{button, container, divider, grid, Button, Grid, Space};
use cosmic::{app, applet::cosmic_panel_config::PanelAnchor, Command};
use cosmic::{
    widget::{icon, rectangle_tracker::*},
    Element, Theme,
};

use chrono::{DateTime, Datelike, Local, Timelike, Weekday};
use std::time::Duration;

use crate::fl;
use crate::time::get_calender_first;
use cosmic::applet::token::subscription::{
    activation_token_subscription, TokenRequest, TokenUpdate,
};

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Every {
    Minute,
    Second,
}

pub struct Window {
    core: cosmic::app::Core,
    popup: Option<window::Id>,
    update_at: Every,
    now: DateTime<Local>,
    rectangle_tracker: Option<RectangleTracker<u32>>,
    rectangle: Rectangle,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
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
    OpenDateTimeSettings,
    Token(TokenUpdate),
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
            Self {
                core,
                popup: None,
                update_at: Every::Minute,
                now: Local::now(),
                rectangle_tracker: None,
                rectangle: Rectangle::default(),
                token_tx: None,
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
            activation_token_subscription(0).map(Message::Token),
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
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        window::Id::MAIN,
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
            Message::SelectDay(_day) => {
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
            Message::OpenDateTimeSettings => {
                let exec = "cosmic-settings time".to_string();
                if let Some(tx) = self.token_tx.as_ref() {
                    let _ = tx.send(TokenRequest {
                        app_id: Self::APP_ID.to_string(),
                        exec,
                    });
                } else {
                    tracing::error!("Wayland tx is None");
                };
                Command::none()
            }
            Message::Token(u) => {
                match u {
                    TokenUpdate::Init(tx) => {
                        self.token_tx = Some(tx);
                    }
                    TokenUpdate::Finished => {
                        self.token_tx = None;
                    }
                    TokenUpdate::ActivationToken { token, .. } => {
                        let mut cmd = std::process::Command::new("cosmic-settings");
                        cmd.arg("time");
                        if let Some(token) = token {
                            cmd.env("XDG_ACTIVATION_TOKEN", &token);
                            cmd.env("DESKTOP_STARTUP_ID", &token);
                        }
                        cosmic::process::spawn(cmd);
                    }
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
                Element::from(
                    cosmic::widget::text(self.now.format("%b %-d %-I:%M %p").to_string()).size(14),
                )
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
                for d in self.now.format("%x").to_string().split('/') {
                    date_time_col = date_time_col.push(text(d.to_string()).size(14));
                }
                date_time_col.into()
            },
        )
        .on_press(Message::TogglePopup)
        .style(cosmic::theme::Button::AppletIcon);

        if let Some(tracker) = self.rectangle_tracker.as_ref() {
            tracker.container(0, button).ignore_bounds(true).into()
        } else {
            button.into()
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let date = text(self.now.format("%B %-d, %Y").to_string()).size(18);
        let day_of_week = text(self.now.format("%A").to_string()).size(14);

        let month_controls = row![
            button::icon(icon::from_name("go-previous-symbolic"))
                .padding([0, 12])
                .on_press(Message::PreviousMonth),
            button::icon(icon::from_name("go-next-symbolic"))
                .padding([0, 12])
                .on_press(Message::NextMonth)
        ];

        // Calender
        let mut calender: Grid<'_, Message> = grid().width(Length::Fill);
        let mut first_day_of_week = Weekday::Sun; // TODO: Configurable
        for _ in 0..7 {
            calender = calender.push(
                text(first_day_of_week)
                    .size(12)
                    .width(Length::Fixed(36.0))
                    .horizontal_alignment(Horizontal::Center),
            );

            first_day_of_week = first_day_of_week.succ();
        }
        calender = calender.insert_row();

        let monday = get_calender_first(self.now.year(), self.now.month(), first_day_of_week);
        let mut day_iter = monday.iter_days();
        for i in 0..35 {
            if i > 0 && i % 7 == 0 {
                calender = calender.insert_row();
            }

            let date = day_iter.next().unwrap();
            let is_month = date.month() == self.now.month() && date.year_ce() == self.now.year_ce();
            let is_day = date.day() == self.now.day() && is_month;

            calender = calender.push(date_button(date.day(), is_month, is_day));
        }

        // content
        let content_list = column![
            row![
                column![date, day_of_week],
                Space::with_width(Length::Fill),
                month_controls,
            ]
            .padding([12, 20]),
            calender.padding([0, 12].into()),
            padded_control(divider::horizontal::default()),
            menu_button(text(fl!("datetime-settings")).size(14))
                .on_press(Message::OpenDateTimeSettings),
        ]
        .padding([8, 0]);

        self.core
            .applet
            .popup_container(container(content_list))
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
) -> Button<'static, Message, cosmic::Theme, cosmic::Renderer> {
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
