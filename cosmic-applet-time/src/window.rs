// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::str::FromStr;
use std::time::Duration;

use cosmic::applet::{menu_button, padded_control};
use cosmic::cctk::sctk::reexports::calloop;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::{
    widget::{column, row, text, vertical_space},
    window, Alignment, Length, Rectangle, Subscription,
};
use cosmic::iced_core::alignment::{Horizontal, Vertical};
use cosmic::iced_style::application;
use cosmic::prelude::CollectionWidget;
use cosmic::widget::{
    button, container, divider, grid, horizontal_space, Button, Column, Grid, Space,
};
use cosmic::{app, applet::cosmic_panel_config::PanelAnchor, Command};
use cosmic::{
    widget::{icon, rectangle_tracker::*},
    Element, Theme,
};

use chrono::{DateTime, Datelike, Local, Locale, Months, NaiveDate, Weekday};

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

impl Every {
    fn from_show_sec(show_sec: bool) -> Self {
        if show_sec {
            Every::Second
        } else {
            Every::Minute
        }
    }

    fn to_duration(&self) -> Duration {
        match self {
            Every::Minute => Duration::from_secs(60),
            Every::Second => Duration::from_secs(1),
        }
    }
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
    locale: Locale,
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

        // XXX: https://github.com/pop-os/cosmic-applets/issues/401
        fn get_local() -> Result<Locale, Box<dyn std::error::Error>> {
            let locale = std::env::var("LANG")?;
            let locale = locale
                .split(".")
                .next()
                .ok_or(format!("Can't split the locale {locale}"))?;
            let locale = Locale::from_str(&locale).map_err(|e| format!("{e:?}"))?;
            Ok(locale)
        }

        let locale = match get_local() {
            Ok(locale) => locale,
            Err(e) => {
                tracing::error!("can't get locale {e}");
                Locale::default()
            }
        };

        let config = TimeAppletConfig::default();

        (
            Self {
                core,
                popup: None,
                update_at: Every::from_show_sec(config.show_seconds),
                now,
                date_selected: NaiveDate::from(now.naive_local()),
                rectangle_tracker: None,
                rectangle: Rectangle::default(),
                token_tx: None,
                config,
                locale,
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
            cosmic::iced::time::every(self.update_at.to_duration()).map(|_| Message::Tick),
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
                self.update_at = Every::from_show_sec(c.show_seconds);
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
            let time = match (self.config.military_time, self.config.show_seconds) {
                (true, true) => "%-I:%M:%S %p",
                (true, false) => "%-I:%M %p",
                (false, true) => "%H:%M:%S",
                (false, false) => "%H:%M",
            };

            let format = if self.config.show_date_in_top_panel {
                if self.config.day_before_month {
                    format!("%d %b {time}")
                } else {
                    format!("%b %d {time}")
                }
            } else {
                time.to_owned()
            };

            Element::from(
                row!(
                    cosmic::widget::text(self.format_time(&self.now, &format)).size(14),
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
                Column::new()
                    .push(text(self.format_time(&self.now, "%H")).size(14))
                    .push(text(self.format_time(&self.now, "%M")).size(14))
                    .push_maybe(
                        self.config
                            .show_seconds
                            .then_some(text(self.format_time(&self.now, "%S")).size(14)),
                    )
            } else {
                Column::new()
                    .push(text(self.format_time(&self.now, "%I")).size(14))
                    .push(text(self.format_time(&self.now, "%M")).size(14))
                    .push_maybe(
                        self.config
                            .show_seconds
                            .then_some(text(self.format_time(&self.now, "%S")).size(14)),
                    )
                    .push(text(self.format_time(&self.now, "%p")).size(14))
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
                for d in self.format_time(&self.now, "%x").split('/') {
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
        let date_format = if self.config.day_before_month {
            "%-d %B %Y"
        } else {
            "%B %-d, %Y"
        };

        let date = text(self.format_date(&self.date_selected, date_format)).size(18);
        let day_of_week = text(self.format_date(&self.date_selected, "%A")).size(14);

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
                text(weekday_localized(&first_day_of_week))
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

        let date = if self.config.day_before_month {
            column![day_of_week, date]
        } else {
            column![date, day_of_week]
        };

        // content
        let content_list = column![
            row![date, Space::with_width(Length::Fill), month_controls,].padding([12, 20]),
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

impl Window {
    fn format_time<'a>(&self, time: &DateTime<Local>, fmt: &'a str) -> String {
        time.format_localized(fmt, self.locale).to_string()
    }

    fn format_date<'a>(&self, time: &NaiveDate, fmt: &'a str) -> String {
        time.format_localized(fmt, self.locale).to_string()
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

fn weekday_localized(weekday: &Weekday) -> String {
    match weekday {
        Weekday::Mon => fl!("mon"),
        Weekday::Tue => fl!("tue"),
        Weekday::Wed => fl!("wed"),
        Weekday::Thu => fl!("thu"),
        Weekday::Fri => fl!("fri"),
        Weekday::Sat => fl!("sat"),
        Weekday::Sun => fl!("sun"),
    }
}
