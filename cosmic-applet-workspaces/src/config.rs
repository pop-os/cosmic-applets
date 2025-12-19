// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::cosmic_config::{
    self, Config, ConfigGet, ConfigSet, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry,
};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "com.system76.CosmicWorkspacesApplet";

#[derive(Default, Debug, Clone, Serialize, Deserialize, CosmicConfigEntry, PartialEq, Eq)]
#[version = 1]
pub struct WorkspacesAppletConfig {
    pub number_format: Option<u8>,
}

impl WorkspacesAppletConfig {
    pub fn current_number_format() -> u8 {
        Config::new(APP_ID, 1)
            .ok()
            .and_then(|c| c.get::<u8>("number_format").ok())
            .unwrap_or(0u8)
    }

    pub fn write_number_format(value: u8) -> Result<(), cosmic_config::Error> {
        let cfg = Config::new(APP_ID, 1)?;
        cfg.set("number_format", value)
    }
}
