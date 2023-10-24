use cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic::cosmic_config::{self, Config, ConfigGet, ConfigSet, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize, CosmicConfigEntry, PartialEq, Eq)]
pub struct AudioAppletConfig {
    pub show_media_controls_in_top_panel: bool,
}

impl AudioAppletConfig {
    /// Returns the version of the config
    pub fn version() -> u64 {
        1
    }
}
