// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic::cosmic_config::{self, CosmicConfigEntry};

#[derive(Debug, Clone, CosmicConfigEntry, PartialEq, Eq)]
#[version = 1]
pub struct TimeAppletConfig {
    pub military_time: bool,
    pub first_day_of_week: u8,
    pub show_date_in_top_panel: bool,
    pub show_seconds: bool,
    pub day_before_month: bool,
}

impl Default for TimeAppletConfig {
    fn default() -> Self {
        Self {
            military_time: false,
            first_day_of_week: 6,
            show_date_in_top_panel: true,
            show_seconds: false,
            day_before_month: false,
        }
    }
}
