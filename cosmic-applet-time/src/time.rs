// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use chrono::{Datelike, Days, NaiveDate, Weekday};

/// Gets the first date that will be visible on the calendar
pub fn get_calendar_first(date_selected: NaiveDate, from_weekday: Weekday) -> NaiveDate {
    let date = date_selected.with_day(1).unwrap();
    let num_days = date.weekday().days_since(from_weekday);
    date.checked_sub_days(Days::new(num_days.into())).unwrap()
}
