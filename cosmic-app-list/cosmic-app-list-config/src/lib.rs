// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::cosmic_config::{
    self, cosmic_config_derive::CosmicConfigEntry, Config, CosmicConfigEntry,
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
pub const APP_ID: &str = "com.system76.CosmicAppList";

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub enum TopLevelFilter {
    #[default]
    ActiveWorkspace,
    ConfiguredOutput,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, CosmicConfigEntry)]
#[version = 1]
pub struct AppListConfig {
    pub filter_top_levels: Option<TopLevelFilter>,
    pub favorites: Vec<String>,
    pub enable_drag_source: bool,
}

impl Default for AppListConfig {
    fn default() -> Self {
        Self {
            filter_top_levels: None,
            favorites: Vec::new(),
            enable_drag_source: true,
        }
    }
}

impl AppListConfig {
    pub fn add_pinned(&mut self, id: String, config: &Config) {
        if !self.favorites.contains(&id) {
            self.favorites.push(id);
            let _ = self.write_entry(config);
        }
    }

    pub fn remove_pinned(&mut self, id: &str, config: &Config) {
        if let Some(pos) = self.favorites.iter().position(|e| e == &id) {
            self.favorites.remove(pos);
            let _ = self.write_entry(config);
        }
    }

    pub fn update_pinned(&mut self, favorites: Vec<String>, config: &Config) {
        self.favorites = favorites;
        let _ = self.write_entry(config);
    }
}
