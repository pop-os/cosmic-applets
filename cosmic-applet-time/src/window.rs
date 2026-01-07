// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use chrono::{Datelike, Timelike};
use cosmic::iced_futures::stream;
use cosmic::widget::Id;
use cosmic::{
    Apply, Element, Task, app,
    applet::{cosmic_panel_config::PanelAnchor, menu_button, padded_control},
    cctk::sctk::reexports::calloop,
    cosmic_theme::Spacing,
    iced::{
        Alignment, Length, Rectangle, Subscription,
        futures::{SinkExt, StreamExt, channel::mpsc},
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        widget::{column, row, vertical_space},
        window,
    },
    iced_widget::{Column, horizontal_rule},
    surface, theme,
    widget::{
        Button, Grid, Space, autosize, button, container, divider, grid, horizontal_space, icon,
        rectangle_tracker::*, text,
    },
};
use logind_zbus::manager::ManagerProxy;
use std::sync::LazyLock;
use timedate_zbus::TimeDateProxy;
use tokio::{sync::watch, time};

use crate::{config::TimeAppletConfig, fl, time::get_calendar_first};
use cosmic::applet::token::subscription::{
    TokenRequest, TokenUpdate, activation_token_subscription,
};
use icu::{
    datetime::{
        DateTimeFormatter, DateTimeFormatterPreferences, fieldsets,
        input::{Date, DateTime, Time},
        options::TimePrecision,
    },
    locale::{Locale, preferences::extensions::unicode::keywords::HourCycle},
};

static AUTOSIZE_MAIN_ID: LazyLock<Id> = LazyLock::new(|| Id::new("autosize-main"));

// Specifiers for strftime that indicate seconds. Subsecond precision isn't supported by the applet
// so those specifiers aren't listed here. This list is non-exhaustive, and it's possible that %X
// and other specifiers have to be added depending on locales.
const STRFTIME_SECONDS: &[char] = &['S', 'T', '+', 's'];

fn get_system_locale() -> Locale {
    for var in ["LC_TIME", "LC_ALL", "LANG"] {
        if let Ok(locale_str) = std::env::var(var) {
            let cleaned_locale = locale_str
                .split('.')
                .next()
                .unwrap_or(&locale_str)
                .replace('_', "-");

            if let Ok(locale) = Locale::try_from_str(&cleaned_locale) {
                return locale;
            }

            // Try language-only fallback (e.g., "en" from "en-US")
            if let Some(lang) = cleaned_locale.split('-').next() {
                if let Ok(locale) = Locale::try_from_str(lang) {
                    return locale;
                }
            }
        }
    }
    tracing::warn!("No valid locale found in environment, using fallback");
    Locale::try_from_str("en-US").expect("Failed to parse fallback locale 'en-US'")
}

pub struct Window {
    core: cosmic::app::Core,
    popup: Option<window::Id>,
    now: chrono::DateTime<chrono::FixedOffset>,
    timezone: Option<chrono_tz::Tz>,
    date_today: chrono::NaiveDate,
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
    Surface(surface::Action),
}

impl Window {
    fn create_datetime<D: Datelike>(&self, date: &D) -> DateTime<icu::calendar::Gregorian> {
        DateTime {
            date: Date::try_new_gregorian(date.year(), date.month() as u8, date.day() as u8)
                .unwrap(),
            time: Time::try_new(
                self.now.hour() as u8,
                self.now.minute() as u8,
                self.now.second() as u8,
                0,
            )
            .unwrap(),
        }
    }

    fn calendar_grid(&self) -> Grid<'_, Message> {
        let mut calendar: Grid<'_, Message> = grid().width(Length::Fill);
        let mut first_day_of_week = chrono::Weekday::try_from(self.config.first_day_of_week)
            .unwrap_or(chrono::Weekday::Sun);

        let first_day = get_calendar_first(
            self.date_selected.year(),
            self.date_selected.month(),
            first_day_of_week,
        );

        let day_iter = first_day.iter_days();
        let prefs = DateTimeFormatterPreferences::from(self.locale.clone());
        let weekday = DateTimeFormatter::try_new(prefs, fieldsets::E::short()).unwrap();

        for date in day_iter.take(7) {
            let datetime = self.create_datetime(&date);
            calendar = calendar.push(
                text::caption(weekday.format(&datetime).to_string())
                    .apply(container)
                    .center_x(Length::Fixed(44.0)),
            );
            first_day_of_week = first_day_of_week.succ();
        }
        calendar = calendar.insert_row();

        let mut day_iter = first_day.iter_days();
        for i in 0..42 {
            if i > 0 && i % 7 == 0 {
                calendar = calendar.insert_row();
            }

            let date = day_iter.next().unwrap();
            let is_month = date.month() == self.date_selected.month()
                && date.year_ce() == self.date_selected.year_ce();
            let is_day = date.day() == self.date_selected.day() && is_month;
            let is_today = date == self.date_today;

            calendar = calendar.push(date_button(date.day(), is_month, is_day, is_today));
        }

        calendar
    }

    /// Format with strftime if non-empty and ignore errors.
    ///
    /// Do not use to_string(). The formatter panics on invalid specifiers.
    fn maybe_strftime(&self) -> Option<String> {
        // strftime may override locale specific elements so it stands alone rather
        // than using ICU.
        (!self.config.format_strftime.is_empty())
            .then(|| {
                let mut s = String::new();
                self.now
                    .format(&self.config.format_strftime)
                    .write_to(&mut s)
                    .map(|_| s)
                    .ok()
            })
            .flatten()
    }

    fn vertical_layout(&self) -> Element<'_, Message> {
        let elements: Vec<Element<'_, Message>> = if let Some(strftime) = self.maybe_strftime() {
            strftime
                .split_whitespace()
                .map(|piece| self.core.applet.text(piece.to_owned()).into())
                .collect()
        } else {
            let mut elements = Vec::new();
            let date = self.now.naive_local();
            let datetime = self.create_datetime(&date);
            let mut prefs = DateTimeFormatterPreferences::from(self.locale.clone());
            prefs.hour_cycle = Some(if self.config.military_time {
                HourCycle::H23
            } else {
                HourCycle::H12
            });

            if self.config.show_date_in_top_panel {
                let formatted_date = DateTimeFormatter::try_new(prefs, fieldsets::MD::medium())
                    .unwrap()
                    .format(&datetime)
                    .to_string();

                for p in formatted_date.split_whitespace() {
                    elements.push(self.core.applet.text(p.to_owned()).into());
                }
                elements.push(
                    horizontal_rule(2)
                        .width(self.core.applet.suggested_size(true).0)
                        .into(),
                );
            }
            let mut fs = fieldsets::T::medium();
            if !self.config.show_seconds {
                fs = fs.with_time_precision(TimePrecision::Minute);
            }
            let formatted_time = DateTimeFormatter::try_new(prefs, fs)
                .unwrap()
                .format(&datetime)
                .to_string();

            // todo: split using formatToParts when it is implemented
            // https://github.com/unicode-org/icu4x/issues/4936#issuecomment-2128812667
            for p in formatted_time.split_whitespace().flat_map(|s| s.split(':')) {
                elements.push(self.core.applet.text(p.to_owned()).into());
            }

            elements
        };

        let date_time_col = Column::with_children(elements)
            .align_x(Alignment::Center)
            .spacing(4);

        Element::from(
            column!(
                date_time_col,
                horizontal_space().width(Length::Fixed(
                    (self.core.applet.suggested_size(true).0
                        + 2 * self.core.applet.suggested_padding(true).1)
                        as f32
                ))
            )
            .align_x(Alignment::Center),
        )
    }

    fn horizontal_layout(&self) -> Element<'_, Message> {
        let formatted_date = if let Some(strftime) = self.maybe_strftime() {
            strftime
        } else {
            let datetime = self.create_datetime(&self.now);
            let mut prefs = DateTimeFormatterPreferences::from(self.locale.clone());
            prefs.hour_cycle = Some(if self.config.military_time {
                HourCycle::H23
            } else {
                HourCycle::H12
            });

            if self.config.show_date_in_top_panel {
                if self.config.show_weekday {
                    let mut fs = fieldsets::MDET::medium();
                    if !self.config.show_seconds {
                        fs = fs.with_time_precision(TimePrecision::Minute);
                    }
                    DateTimeFormatter::try_new(prefs, fs)
                        .unwrap()
                        .format(&datetime)
                        .to_string()
                } else {
                    let mut fs = fieldsets::MDT::medium();
                    if !self.config.show_seconds {
                        fs = fs.with_time_precision(TimePrecision::Minute);
                    }
                    DateTimeFormatter::try_new(prefs, fs)
                        .unwrap()
                        .format(&datetime)
                        .to_string()
                }
            } else {
                let mut fs = fieldsets::T::medium();
                if !self.config.show_seconds {
                    fs = fs.with_time_precision(TimePrecision::Minute);
                }
                DateTimeFormatter::try_new(prefs, fs)
                    .unwrap()
                    .format(&datetime)
                    .to_string()
            }
        };

        Element::from(
            row!(
                self.core.applet.text(formatted_date),
                container(vertical_space().height(Length::Fixed(
                    (self.core.applet.suggested_size(true).1
                        + 2 * self.core.applet.suggested_padding(true).1)
                        as f32
                )))
            )
            .align_y(Alignment::Center),
        )
    }
}

impl cosmic::Application for Window {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletTime";

    fn init(core: app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        let locale = get_system_locale();

        // Chrono evaluates the local timezone once whereby it's stored in a thread local
        // variable but never updated
        // Instead of using the local timezone, we will store an offset that is updated if the
        // timezone is ever externally changed
        let now = chrono::Local::now().fixed_offset();

        // get today's date for highlighting purposes
        let today = chrono::NaiveDate::from(now.naive_local());

        // Synch `show_seconds` from the config within the time subscription
        let (show_seconds_tx, _) = watch::channel(true);

        (
            Self {
                core,
                popup: None,
                now,
                timezone: None,
                date_today: today,
                date_selected: today,
                rectangle_tracker: None,
                rectangle: Rectangle::default(),
                token_tx: None,
                config: TimeAppletConfig::default(),
                show_seconds_tx,
                locale,
            },
            Task::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Message> {
        fn time_subscription(mut show_seconds: watch::Receiver<bool>) -> Subscription<Message> {
            Subscription::run_with_id(
                "time-sub",
                stream::channel(1, |mut output| async move {
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
                }),
            )
        }

        // Update applet's timezone if the system's timezone changes
        async fn timezone_update(output: &mut mpsc::Sender<Message>) -> zbus::Result<()> {
            let conn = zbus::Connection::system().await?;
            let proxy = TimeDateProxy::new(&conn).await?;

            // The stream always returns the current timezone as its first item even if it wasn't
            // updated. If the proxy is recreated in a loop somehow, the resulting stream will
            // always yield an update immediately which could lead to spammed false updates.
            let mut stream_tz = proxy.receive_timezone_changed().await;
            while let Some(property) = stream_tz.next().await {
                let tz = property.get().await?;
                output
                    .send(Message::TimezoneUpdate(tz))
                    .await
                    .map_err(|e| {
                        zbus::Error::InputOutput(std::sync::Arc::new(std::io::Error::other(e)))
                    })?;
            }
            Ok(())
        }

        fn timezone_subscription() -> Subscription<Message> {
            Subscription::run_with_id(
                "timezone-sub",
                stream::channel(1, |mut output| async move {
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
                }),
            )
        }

        // Update the time when waking from sleep
        async fn wake_from_sleep(output: &mut mpsc::Sender<Message>) -> zbus::Result<()> {
            let connection = zbus::Connection::system().await?;
            let proxy = ManagerProxy::new(&connection).await?;

            while let Some(property) = proxy.receive_prepare_for_sleep().await?.next().await {
                let waking = !property.args()?.start();
                if waking {
                    let _ = output.send(Message::Tick).await;
                }
            }
            Ok(())
        }

        fn wake_from_sleep_subscription() -> Subscription<Message> {
            Subscription::run_with_id(
                "wake-from-suspend-sub",
                stream::channel(1, |mut output| async move {
                    if let Err(err) = wake_from_sleep(&mut output).await {
                        tracing::error!(?err, "Failed to subscribe to wake-from-sleep signal");
                    }
                }),
            )
        }

        let show_seconds_rx = self.show_seconds_tx.subscribe();
        Subscription::batch([
            rectangle_tracker_subscription(0).map(|e| Message::Rectangle(e.1)),
            time_subscription(show_seconds_rx),
            activation_token_subscription(0).map(Message::Token),
            timezone_subscription(),
            wake_from_sleep_subscription(),
            self.core.watch_config(Self::APP_ID).map(|u| {
                for err in u.errors {
                    tracing::error!(?err, "Error watching config");
                }
                Message::ConfigChanged(u.config)
            }),
        ])
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    self.date_today = chrono::NaiveDate::from(self.now.naive_local());
                    self.date_selected = self.date_today;

                    let new_id = window::Id::unique();
                    self.popup = Some(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
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
                        x: x.max(1.) as i32,
                        y: y.max(1.) as i32,
                        width: width.max(1.) as i32,
                        height: height.max(1.) as i32,
                    };

                    popup_settings.positioner.size = None;

                    get_popup(popup_settings)
                }
            }
            Message::Tick => {
                self.now = self.timezone.map_or_else(
                    || chrono::Local::now().into(),
                    |tz| chrono::Local::now().with_timezone(&tz).fixed_offset(),
                );
                Task::none()
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
                Task::none()
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
                Task::none()
            }
            Message::SelectDay(day) => {
                if let Some(date) = self.date_selected.with_day(day) {
                    self.date_selected = date;
                } else {
                    tracing::error!("invalid naivedate");
                }
                Task::none()
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
                Task::none()
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
                Task::none()
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
                }
                Task::none()
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
                Task::none()
            }
            Message::ConfigChanged(c) => {
                // Don't interrupt the tick subscription unless necessary
                self.show_seconds_tx.send_if_modified(|show_seconds| {
                    if !c.format_strftime.is_empty() {
                        if c.format_strftime.split('%').any(|s| {
                            STRFTIME_SECONDS.contains(&s.chars().next().unwrap_or_default())
                        }) && !*show_seconds
                        {
                            // The strftime formatter contains a seconds specifier. Force enable
                            // ticking per seconds internally regardless of the user setting.
                            // This does not change the user's setting. It's invisible to the user.
                            *show_seconds = true;
                            true
                        } else {
                            false
                        }
                    } else if *show_seconds == c.show_seconds {
                        false
                    } else {
                        *show_seconds = c.show_seconds;
                        true
                    }
                });
                self.config = c;
                Task::none()
            }
            Message::TimezoneUpdate(timezone) => {
                if let Ok(timezone) = timezone.parse::<chrono_tz::Tz>() {
                    self.now = chrono::Local::now().with_timezone(&timezone).fixed_offset();
                    self.date_today = chrono::NaiveDate::from(self.now.naive_local());
                    self.date_selected = self.date_today;
                    self.timezone = Some(timezone);
                }

                self.update(Message::Tick)
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let horizontal = matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        );

        let button = button::custom(if horizontal {
            self.horizontal_layout()
        } else {
            self.vertical_layout()
        })
        .padding(if horizontal {
            [0, self.core.applet.suggested_padding(true).0]
        } else {
            [self.core.applet.suggested_padding(true).0, 0]
        })
        .on_press_down(Message::TogglePopup)
        .class(cosmic::theme::Button::AppletIcon);

        autosize::autosize(
            if let Some(tracker) = self.rectangle_tracker.as_ref() {
                Element::from(tracker.container(0, button).ignore_bounds(true))
            } else {
                button.into()
            },
            AUTOSIZE_MAIN_ID.clone(),
        )
        .into()
    }

    fn view_window(&self, _id: window::Id) -> Element<'_, Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let datetime = self.create_datetime(&self.date_selected);
        let prefs = DateTimeFormatterPreferences::from(self.locale.clone());

        let date = text(
            DateTimeFormatter::try_new(prefs, fieldsets::YMD::long())
                .unwrap()
                .format(&datetime)
                .to_string(),
        )
        .size(18);
        let day_of_week = text::body(
            DateTimeFormatter::try_new(prefs, fieldsets::E::long())
                .unwrap()
                .format(&datetime)
                .to_string(),
        );

        let month_controls = row![
            button::icon(icon::from_name("go-previous-symbolic"))
                .padding(8)
                .on_press(Message::PreviousMonth),
            button::icon(icon::from_name("go-next-symbolic"))
                .padding(8)
                .on_press(Message::NextMonth)
        ]
        .spacing(8);

        let calendar = self.calendar_grid();

        let content_list = column![
            row![
                column![date, day_of_week],
                Space::with_width(Length::Fill),
                month_controls,
            ]
            .align_y(Alignment::Center)
            .padding([12, 20]),
            calendar.padding([0, 12].into()),
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
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

fn date_button(day: u32, is_month: bool, is_day: bool, is_today: bool) -> Button<'static, Message> {
    let style = if is_day {
        button::ButtonClass::Suggested
    } else if is_today {
        button::ButtonClass::Standard
    } else {
        button::ButtonClass::Text
    };

    let button = button::custom(
        text::body(format!("{day}"))
            .apply(container)
            .center(Length::Fill),
    )
    .class(style)
    .height(Length::Fixed(44.0))
    .width(Length::Fixed(44.0));

    if is_month {
        button.on_press(Message::SelectDay(day))
    } else {
        button
    }
}
