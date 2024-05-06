// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::applet::{menu_button, padded_control};
use cosmic::cctk::sctk::reexports::calloop;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::{
    subscription,
    widget::{column, row, text, vertical_space},
    window, Alignment, Length, Rectangle, Subscription,
};
use cosmic::iced_core::alignment::{Horizontal, Vertical};
use cosmic::iced_style::application;
use cosmic::widget::{button, container, divider, grid, horizontal_space, Button, Grid, Space};
use cosmic::{app, applet::cosmic_panel_config::PanelAnchor, Command};
use cosmic::{
    widget::{icon, rectangle_tracker::*},
    Element, Theme,
};

use chrono::{DateTime, Datelike, DurationRound, Local, Months, NaiveDate, Weekday};

use crate::config::TimeAppletConfig;
use crate::fl;
use crate::time::get_calender_first;
use cosmic::applet::token::subscription::{
    activation_token_subscription, TokenRequest, TokenUpdate,
};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum Every {
    Minute,
    Second,
}

pub struct Window {
    core: cosmic::app::Core,
    popup: Option<window::Id>,
    update_at: Every,
    now: DateTime<Local>,
    date_selected: NaiveDate,
    rectangle_tracker: Option<RectangleTracker<u32>>,
    rectangle: Rectangle,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    config: TimeAppletConfig,
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
    ConfigChanged(TimeAppletConfig),
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
        let now = Local::now();
        (
            Self {
                core,
                popup: None,
                update_at: Every::Minute,
                now,
                date_selected: NaiveDate::from(now.naive_local()),
                rectangle_tracker: None,
                rectangle: Rectangle::default(),
                token_tx: None,
                config: TimeAppletConfig::default(),
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
        Subscription::batch(vec![
            rectangle_tracker_subscription(0).map(|e| Message::Rectangle(e.1)),
            time_subscription(self.update_at).map(|_| Message::Tick),
            activation_token_subscription(0).map(Message::Token),
            self.core.watch_config(Self::APP_ID).map(|u| {
                for err in u.errors {
                    tracing::error!(?err, "Error watching config");
                }
                Message::ConfigChanged(u.config)
            }),
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
                    self.date_selected = NaiveDate::from(self.now.naive_local());

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
                if let Some(date) = self.date_selected.with_day(_day) {
                    self.date_selected = date;
                } else {
                    tracing::error!("invalid naivedate");
                }
                Command::none()
            }
            Message::PreviousMonth => {
                if let Some(date) = self.date_selected.checked_sub_months(Months::new(1)) {
                    self.date_selected = date;
                } else {
                    tracing::error!("invalid naivedate");
                }
                Command::none()
            }
            Message::NextMonth => {
                if let Some(date) = self.date_selected.checked_add_months(Months::new(1)) {
                    self.date_selected = date;
                } else {
                    tracing::error!("invalid naivedate");
                }
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
            Message::ConfigChanged(c) => {
                self.config = c;
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let horizontal = matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        );
        let button = cosmic::widget::button(if horizontal {
            let format = match (
                self.config.military_time,
                self.config.show_date_in_top_panel,
            ) {
                (true, true) => "%b %-d %H:%M",
                (true, false) => "%H:%M",
                (false, true) => "%b %-d %-I:%M %p",
                (false, false) => "%-I:%M %p",
            };
            Element::from(
                row!(
                    cosmic::widget::text(self.now.format(format).to_string()).size(14),
                    container(vertical_space(Length::Fixed(
                        (self.core.applet.suggested_size(true).1
                            + 2 * self.core.applet.suggested_padding(true))
                            as f32
                    )))
                )
                .align_items(Alignment::Center),
            )
        } else {
            let mut date_time_col = if self.config.military_time {
                column![
                    text(self.now.format("%H").to_string()).size(14),
                    text(self.now.format("%M").to_string()).size(14),
                ]
            } else {
                column![
                    text(self.now.format("%I").to_string()).size(14),
                    text(self.now.format("%M").to_string()).size(14),
                    text(self.now.format("%p").to_string()).size(14),
                ]
            }
            .align_items(Alignment::Center)
            .spacing(4);
            if self.config.show_date_in_top_panel {
                date_time_col = date_time_col.push(vertical_space(Length::Fixed(4.0)));
                date_time_col = date_time_col.push(
                    // TODO better calendar icon?
                    icon::from_name("calendar-go-today-symbolic")
                        .size(self.core.applet.suggested_size(true).0)
                        .symbolic(true),
                );
                for d in self.now.format("%x").to_string().split('/') {
                    date_time_col = date_time_col.push(text(d.to_string()).size(14));
                }
            }
            Element::from(
                column!(
                    date_time_col,
                    horizontal_space(Length::Fixed(
                        (self.core.applet.suggested_size(true).0
                            + 2 * self.core.applet.suggested_padding(true))
                            as f32
                    ))
                )
                .align_items(Alignment::Center),
            )
        })
        .padding(if horizontal {
            [0, self.core.applet.suggested_padding(true)]
        } else {
            [self.core.applet.suggested_padding(true), 0]
        })
        .on_press(Message::TogglePopup)
        .style(cosmic::theme::Button::AppletIcon);

        if let Some(tracker) = self.rectangle_tracker.as_ref() {
            tracker.container(0, button).ignore_bounds(true).into()
        } else {
            button.into()
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let date = text(self.date_selected.format("%B %-d, %Y").to_string()).size(18);
        let day_of_week = text(self.date_selected.format("%A").to_string()).size(14);

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
        let mut first_day_of_week =
            Weekday::try_from(self.config.first_day_of_week).unwrap_or(Weekday::Sun);
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

        let monday = get_calender_first(
            self.date_selected.year(),
            self.date_selected.month(),
            first_day_of_week,
        );
        let mut day_iter = monday.iter_days();
        for i in 0..42 {
            if i > 0 && i % 7 == 0 {
                calender = calender.insert_row();
            }

            let date = day_iter.next().unwrap();
            let is_month = date.month() == self.date_selected.month()
                && date.year_ce() == self.date_selected.year_ce();
            let is_day = date.day() == self.date_selected.day() && is_month;

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

fn time_subscription(update_at: Every) -> Subscription<()> {
    subscription::unfold("time-sub", (), move |()| async move {
        let now = Local::now();
        let update_delay = match update_at {
            Every::Minute => chrono::TimeDelta::minutes(1),
            Every::Second => chrono::TimeDelta::seconds(1),
        };
        let duration = ((now + update_delay).duration_trunc(update_delay).unwrap() - now)
            .to_std()
            .unwrap();
        tokio::time::sleep(duration).await;
        ((), ())
    })
}
