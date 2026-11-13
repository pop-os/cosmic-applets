// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic_config::{
    self, Config, ConfigGet, ConfigSet, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry,
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
pub const APP_ID: &str = "com.system76.CosmicAppList";

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub enum ToplevelFilter {
    #[default]
    ActiveWorkspace,
    ConfiguredOutput,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, CosmicConfigEntry)]
#[version = 1]
pub struct AppListConfig {
    pub filter_top_levels: Option<ToplevelFilter>,
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
        let mut favorites = self.saved_favorites(config);
        if !favorites.contains(&id) {
            favorites.push(id);
            let _ = config.set("favorites", &favorites);
        }
        self.favorites = favorites;
    }

    pub fn remove_pinned(&mut self, id: &str, config: &Config) {
        let mut favorites = self.saved_favorites(config);
        if let Some(pos) = favorites.iter().position(|e| e == id) {
            favorites.remove(pos);
            let _ = config.set("favorites", &favorites);
        }
        self.favorites = favorites;
    }

    pub fn update_pinned(&mut self, favorites: Vec<String>, config: &Config) {
        let _ = config.set("favorites", &favorites);
        self.favorites = favorites;
    }

    fn saved_favorites(&self, config: &Config) -> Vec<String> {
        config
            .get("favorites")
            .unwrap_or_else(|_| self.favorites.clone())
    }
}
