// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};

#[derive(Debug, Clone, CosmicConfigEntry, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[version = 1]
pub struct TimeAppletConfig {
    pub military_time: bool,
    pub show_seconds: bool,
    pub first_day_of_week: u8,
    pub show_date_in_top_panel: bool,
    pub show_weekday: bool,
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub format_strftime: String,
    /// Extra time zones shown in the calendar popup.
    ///
    /// Each entry is an IANA time zone id, optionally prefixed with a display
    /// label and a `|` separator, such as `SF|America/Los_Angeles`. Without a
    /// label, the city part of the id is used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub world_clocks: Vec<String>,
}

impl Default for TimeAppletConfig {
    fn default() -> Self {
        Self {
            military_time: false,
            show_seconds: false,
            first_day_of_week: 6,
            show_date_in_top_panel: true,
            show_weekday: false,
            format_strftime: Default::default(),
            world_clocks: Default::default(),
        }
    }
}
