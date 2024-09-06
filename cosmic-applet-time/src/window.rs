// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::{borrow::Cow, str::FromStr};

use chrono::{Datelike, Timelike};
use cosmic::{
    app,
    applet::{cosmic_panel_config::PanelAnchor, menu_button, padded_control},
    cctk::sctk::reexports::calloop,
    iced::{
        futures::{channel::mpsc, SinkExt, StreamExt, TryFutureExt},
        subscription,
        wayland::popup::{destroy_popup, get_popup},
        widget::{column, row, vertical_space},
        window, Alignment, Length, Rectangle, Subscription,
    },
    iced_core::alignment::{Horizontal, Vertical},
    iced_style::application,
    iced_widget::{horizontal_rule, Column},
    widget::{
        button, container, divider, grid, horizontal_space, icon, rectangle_tracker::*, text,
        Button, Grid, Space,
    },
    Command, Element, Theme,
};
use timedate_zbus::TimeDateProxy;
use tokio::{sync::watch, time};

use icu::{
    calendar::DateTime,
    datetime::{
        options::{
            components::{self, Bag},
            preferences,
        },
        DateTimeFormatter, DateTimeFormatterOptions,
    },
    locid::Locale,
};

use crate::{config::TimeAppletConfig, fl, time::get_calender_first};
use cosmic::applet::token::subscription::{
    activation_token_subscription, TokenRequest, TokenUpdate,
};

/// In order to keep the understandable, the chrono types are not globals,
/// to avoid conflict with icu

pub struct Window {
    core: cosmic::app::Core,
    popup: Option<window::Id>,
    now: chrono::DateTime<chrono::FixedOffset>,
    timezone: Option<chrono_tz::Tz>,
    date_selected: chrono::NaiveDate,
    rectangle_tracker: Option<RectangleTracker<u32>>,
    rectangle: Rectangle,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
    config: TimeAppletConfig,
    show_seconds_tx: watch::Sender<bool>,
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
    TimezoneUpdate(String),
}

impl Window {
    fn format<D: Datelike>(&self, bag: Bag, date: &D) -> String {
        let options = DateTimeFormatterOptions::Components(bag);

        let dtf =
            DateTimeFormatter::try_new_experimental(&self.locale.clone().into(), options).unwrap();

        let datetime = DateTime::try_new_gregorian_datetime(
            date.year(),
            date.month() as u8,
            date.day() as u8,
            // hack cause we know that we will only use "now"
            // when we need hours (NaiveDate don't support this functions)
            self.now.hour() as u8,
            self.now.minute() as u8,
            self.now.second() as u8,
        )
        .unwrap()
        .to_iso()
        .to_any();

        dtf.format(&datetime)
            .expect("can't format value")
            .to_string()
    }
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
        fn get_local() -> Result<Locale, Box<dyn std::error::Error>> {
            let locale = std::env::var("LC_TIME").or_else(|_| std::env::var("LANG"))?;
            let locale = locale
                .split('.')
                .next()
                .ok_or(format!("Can't split the locale {locale}"))?;

            let locale = Locale::from_str(locale).map_err(|e| format!("{e:?}"))?;
            Ok(locale)
        }

        let locale = match get_local() {
            Ok(locale) => locale,
            Err(e) => {
                tracing::error!("can't get locale {e}");
                Locale::default()
            }
        };

        // Chrono evaluates the local timezone once whereby it's stored in a thread local
        // variable but never updated
        // Instead of using the local timezone, we will store an offset that is updated if the
        // timezone is ever externally changed
        let now = chrono::Local::now().fixed_offset();

        // Synch `show_seconds` from the config within the time subscription
        let (show_seconds_tx, _) = watch::channel(true);

        (
            Self {
                core,
                popup: None,
                now,
                timezone: None,
                date_selected: chrono::NaiveDate::from(now.naive_local()),
                rectangle_tracker: None,
                rectangle: Rectangle::default(),
                token_tx: None,
                config: TimeAppletConfig::default(),
                show_seconds_tx,
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
        fn time_subscription(mut show_seconds: watch::Receiver<bool>) -> Subscription<Message> {
            subscription::channel("time-sub", 1, |mut output| async move {
                // Mark this receiver's state as changed so that it always receives an initial
                // update during the loop below
                // This allows us to avoid duplicating code from the loop
                show_seconds.mark_changed();
                let mut period = 1;
                let mut timer = time::interval(time::Duration::from_secs(period));
                timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        _ = timer.tick() => {
                            #[cfg(debug_assertions)]
                            if let Err(err) = output.send(Message::Tick).await {
                                tracing::error!(?err, "Failed sending tick request to applet");
                            }
                            #[cfg(not(debug_assertions))]
                            let _ = output.send(Message::Tick).await;

                            // Calculate a delta if we're ticking per minute to keep ticks stable
                            // Based on i3status-rust
                            let current = chrono::Local::now().second() as u64 % period;
                            if current != 0 {
                                timer.reset_after(time::Duration::from_secs(period - current));
                            }
                        },
                        // Update timer if the user toggles show_seconds
                        Ok(()) = show_seconds.changed() => {
                            let seconds = *show_seconds.borrow_and_update();
                            if seconds {
                                period = 1;
                                // Subsecond precision isn't needed; skip calculating offset
                                let period = time::Duration::from_secs(period);
                                let start = time::Instant::now() + period;
                                timer = time::interval_at(start, period);
                            } else {
                                period = 60;
                                let delta = time::Duration::from_secs(period - chrono::Utc::now().second() as u64 % period);
                                let now = time::Instant::now();
                                // Start ticking from the next minute to update the time properly
                                let start = now + delta;
                                let period = time::Duration::from_secs(period);
                                timer = time::interval_at(start, period);
                            }

                            timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
                        }
                    }
                }
            })
        }

        // Update applet's timezone if the system's timezone changes
        async fn timezone_update(output: &mut mpsc::Sender<Message>) -> zbus::Result<()> {
            let conn = zbus::Connection::system().await?;
            let proxy = TimeDateProxy::new(&conn).await?;

            // The stream always returns the current timezone as its first item even if it wasn't
            // updated. If the proxy is recreated in a loop somehow, the resulting stream will
            // always yield an update immediately which could lead to spammed false updates.
            while let Some(property) = proxy.receive_timezone_changed().await.next().await {
                let tz = property.get().await?;
                output
                    .send(Message::TimezoneUpdate(tz))
                    .map_err(|e| {
                        zbus::Error::InputOutput(std::sync::Arc::new(std::io::Error::other(e)))
                    })
                    .await?;
            }

            Ok(())
        }

        fn timezone_subscription() -> Subscription<Message> {
            subscription::channel("timezone-sub", 1, |mut output| async move {
                'retry: loop {
                    match timezone_update(&mut output).await {
                        Ok(()) => break 'retry,
                        Err(err) => {
                            tracing::error!(
                                ?err,
                                "Automatic timezone updater failed; retrying in one minute"
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        }
                    }
                }

                std::future::pending().await
            })
        }

        let show_seconds_rx = self.show_seconds_tx.subscribe();
        Subscription::batch(vec![
            rectangle_tracker_subscription(0).map(|e| Message::Rectangle(e.1)),
            time_subscription(show_seconds_rx),
            activation_token_subscription(0).map(Message::Token),
            timezone_subscription(),
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
                    self.date_selected = chrono::NaiveDate::from(self.now.naive_local());

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
                self.now = self
                    .timezone
                    .map(|tz| chrono::Local::now().with_timezone(&tz).fixed_offset())
                    .unwrap_or_else(|| chrono::Local::now().into());
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
                if let Some(date) = self
                    .date_selected
                    .checked_sub_months(chrono::Months::new(1))
                {
                    self.date_selected = date;
                } else {
                    tracing::error!("invalid naivedate");
                }
                Command::none()
            }
            Message::NextMonth => {
                if let Some(date) = self
                    .date_selected
                    .checked_add_months(chrono::Months::new(1))
                {
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
                        tokio::spawn(cosmic::process::spawn(cmd));
                    }
                }
                Command::none()
            }
            Message::ConfigChanged(c) => {
                // Don't interrupt the tick subscription unless necessary
                self.show_seconds_tx.send_if_modified(|show_seconds| {
                    if *show_seconds == c.show_seconds {
                        false
                    } else {
                        *show_seconds = c.show_seconds;
                        true
                    }
                });
                self.config = c;
                Command::none()
            }
            Message::TimezoneUpdate(timezone) => {
                if let Ok(timezone) = timezone.parse::<chrono_tz::Tz>() {
                    self.now = chrono::Local::now().with_timezone(&timezone).fixed_offset();
                    self.timezone = Some(timezone);
                }

                self.update(Message::Tick)
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let horizontal = matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        );

        let button = cosmic::widget::button(if horizontal {
            let mut bag = Bag::empty();

            if self.config.show_date_in_top_panel {
                if self.config.show_weekday {
                    bag.weekday = Some(components::Text::Short);
                }

                bag.day = Some(components::Day::NumericDayOfMonth);
                bag.month = Some(components::Month::Long);
            }

            bag.hour = Some(components::Numeric::Numeric);
            bag.minute = Some(components::Numeric::Numeric);
            bag.second = self
                .config
                .show_seconds
                .then_some(components::Numeric::Numeric);

            let hour_cycle = if self.config.military_time {
                preferences::HourCycle::H23
            } else {
                preferences::HourCycle::H12
            };

            bag.preferences = Some(preferences::Bag::from_hour_cycle(hour_cycle));

            Element::from(
                row!(
                    self.core.applet.text(self.format(bag, &self.now)),
                    container(vertical_space(Length::Fixed(
                        (self.core.applet.suggested_size(true).1
                            + 2 * self.core.applet.suggested_padding(true))
                            as f32
                    )))
                )
                .align_items(Alignment::Center),
            )
        } else {
            // vertical layout

            let mut elements = Vec::new();

            if self.config.show_date_in_top_panel {
                let mut date_bag = Bag::empty();

                date_bag.day = Some(components::Day::NumericDayOfMonth);
                date_bag.month = Some(components::Month::Short);

                let formated = self.format(date_bag, &self.now);

                for p in formated.split_whitespace() {
                    elements.push(self.core.applet.text(p.to_owned()).into());
                }

                elements.push(
                    horizontal_rule(2)
                        .width(self.core.applet.suggested_size(true).0)
                        .into(),
                )
            }

            let mut time_bag: Bag = Bag::empty();

            time_bag.hour = Some(components::Numeric::Numeric);
            time_bag.minute = Some(components::Numeric::Numeric);

            let hour_cycle = if self.config.military_time {
                preferences::HourCycle::H23
            } else {
                preferences::HourCycle::H12
            };

            time_bag.preferences = Some(preferences::Bag::from_hour_cycle(hour_cycle));

            let formated = self.format(time_bag, &self.now);

            // todo: split using formatToParts when it is implemented
            // https://github.com/unicode-org/icu4x/issues/4936#issuecomment-2128812667
            for p in formated.split_whitespace().flat_map(|s| s.split(':')) {
                elements.push(self.core.applet.text(p.to_owned()).into());
            }

            let date_time_col = Column::with_children(elements)
                .align_items(Alignment::Center)
                .spacing(4);

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
        .on_press_down(Message::TogglePopup)
        .style(cosmic::theme::Button::AppletIcon);

        if let Some(tracker) = self.rectangle_tracker.as_ref() {
            tracker.container(0, button).ignore_bounds(true).into()
        } else {
            button.into()
        }
    }

    fn view_window(&self, _id: window::Id) -> Element<Message> {
        let mut date_bag = Bag::empty();
        date_bag.month = Some(components::Month::Long);
        date_bag.day = Some(components::Day::NumericDayOfMonth);
        date_bag.year = Some(components::Year::Numeric);

        let date = text(self.format(date_bag, &self.date_selected)).size(18);

        let mut day_of_week_bag = Bag::empty();
        day_of_week_bag.weekday = Some(components::Text::Long);

        let day_of_week = text::body(self.format(day_of_week_bag, &self.date_selected));

        let month_controls = row![
            button::icon(icon::from_name("go-previous-symbolic"))
                .padding(8)
                .on_press(Message::PreviousMonth),
            button::icon(icon::from_name("go-next-symbolic"))
                .padding(8)
                .on_press(Message::NextMonth)
        ]
        .spacing(8);

        // Calender

        let mut calender: Grid<'_, Message> = grid().width(Length::Fill);
        let mut first_day_of_week = chrono::Weekday::try_from(self.config.first_day_of_week)
            .unwrap_or(chrono::Weekday::Sun);

        let first_day = get_calender_first(
            self.date_selected.year(),
            self.date_selected.month(),
            first_day_of_week,
        );

        let mut weekday_bag = Bag::empty();
        weekday_bag.weekday = Some(components::Text::Short);

        let mut day_iter = first_day.iter_days();

        for _ in 0..7 {
            calender = calender.push(
                text(self.format(weekday_bag, &day_iter.next().unwrap()))
                    .size(12)
                    .width(Length::Fixed(36.0))
                    .horizontal_alignment(Horizontal::Center),
            );

            first_day_of_week = first_day_of_week.succ();
        }
        calender = calender.insert_row();

        let mut day_iter = first_day.iter_days();
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
            .align_items(Alignment::Center)
            .padding([12, 20]),
            calender.padding([0, 12].into()),
            padded_control(divider::horizontal::default()),
            menu_button(text::body(fl!("datetime-settings")))
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

fn date_button(day: u32, is_month: bool, is_day: bool) -> Button<'static, Message> {
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
