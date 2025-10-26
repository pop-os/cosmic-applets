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
    #[serde(default, deserialize_with = "strftime_opt_de")]
    pub format_strftime: Option<String>,
}

impl Default for TimeAppletConfig {
    fn default() -> Self {
        Self {
            military_time: false,
            show_seconds: false,
            first_day_of_week: 6,
            show_date_in_top_panel: true,
            show_weekday: false,
            format_strftime: None,
        }
    }
}

/// Deserialize optional String but only if it is non-empty.
fn strftime_opt_de<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(|strftime: Option<String>| {
        if strftime.as_deref().is_none_or(str::is_empty) {
            None
        } else {
            strftime
        }
    })
}
