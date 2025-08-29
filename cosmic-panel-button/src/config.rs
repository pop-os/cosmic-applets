use std::collections::HashMap;

use cosmic_config::{cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, CosmicConfigEntry)]
#[version = 1]
#[serde(deny_unknown_fields)]
pub struct CosmicPanelButtonConfig {
    /// configs indexed by panel name
    pub configs: HashMap<String, IndividualConfig>,
}

impl Default for CosmicPanelButtonConfig {
    fn default() -> Self {
        Self {
            configs: HashMap::from([
                (
                    "Panel".to_string(),
                    IndividualConfig {
                        force_presentation: None,
                    },
                ),
                (
                    "Dock".to_string(),
                    IndividualConfig {
                        force_presentation: Some(Override::Icon),
                    },
                ),
            ]),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Default, Clone)]
pub struct IndividualConfig {
    pub force_presentation: Option<Override>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum Override {
    Icon,
    Text,
}
