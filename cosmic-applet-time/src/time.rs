// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use jiff::{
    ToSpan,
    civil::{Date, Weekday},
};

/// Gets the first date that will be visible on the calendar
pub fn get_calendar_first(year: i16, month: i8, from_weekday: Weekday) -> Date {
    let date = Date::new(year, month, 1).expect("valid date");
    let num_days = date.weekday().since(from_weekday);
    date.checked_sub(num_days.days()).expect("valid date")
}

pub const fn offset_to_weekday(offset: u8) -> Weekday {
    match offset {
        0 => Weekday::Monday,
        1 => Weekday::Tuesday,
        2 => Weekday::Wednesday,
        3 => Weekday::Thursday,
        4 => Weekday::Friday,
        5 => Weekday::Saturday,
        _ => Weekday::Sunday,
    }
}
