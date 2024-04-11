use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use cosmic_comp_config::XkbConfig;
use serde::{Deserialize, Serialize};
pub const CONFIG_VERSION: u64 = 1;
#[derive(Clone, CosmicConfigEntry, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Config {}
impl Default for Config {
    fn default() -> Self {
        Self {}
    }
}
#[derive(Clone, CosmicConfigEntry, Debug, Deserialize, PartialEq, Serialize, Default)]
pub struct CosmicCompConfig {
    pub xkb_config: XkbConfig,
}
