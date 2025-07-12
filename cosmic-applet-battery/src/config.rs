// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic_config::{cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "com.system76.CosmicAppletButton";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, CosmicConfigEntry, Default)]
#[version = 1]
pub struct BatteryConfig {
    pub show_percentage: bool,
}
