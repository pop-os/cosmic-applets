use cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic::cosmic_config::{self, CosmicConfigEntry};

#[derive(Debug, Clone, CosmicConfigEntry, PartialEq, Eq)]
#[version = 1]
pub struct TimeAppletConfig {
    pub military_time: bool,
    pub first_day_of_week: u8,
    pub show_date_in_top_panel: bool,
}

impl Default for TimeAppletConfig {
    fn default() -> Self {
        Self {
            military_time: false,
            first_day_of_week: 6,
            show_date_in_top_panel: true,
        }
    }
}
