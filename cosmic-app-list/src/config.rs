use anyhow::anyhow;

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{Config, ConfigGet, ConfigSet, CosmicConfigEntry};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fs::File;
use std::path::PathBuf;
use xdg::BaseDirectories;
pub const APP_ID: &str = "com.system76.CosmicAppList";
pub const VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub enum TopLevelFilter {
    #[default]
    ActiveWorkspace,
    ConfiguredOutput,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, CosmicConfigEntry)]
pub struct AppListConfig {
    pub filter_top_levels: Option<TopLevelFilter>,
    pub favorites: Vec<String>,
}

impl AppListConfig {
    // TODO async?
    /// load config with the provided name
    pub fn load() -> anyhow::Result<AppListConfig> {
        let mut relative_path = PathBuf::from(APP_ID);
        relative_path.push("config.ron");
        let file = match BaseDirectories::new()
            .ok()
            .and_then(|dirs| dirs.find_config_file(relative_path))
            .and_then(|p| File::open(p).ok())
        {
            Some(path) => path,
            _ => {
                anyhow::bail!("Failed to load config");
            }
        };

        ron::de::from_reader::<_, AppListConfig>(file)
            .map_err(|err| anyhow!("Failed to parse config file: {}", err))
    }

    pub fn add_favorite(&mut self, id: String, config: &Config) {
        if !self.favorites.contains(&id) {
            self.favorites.push(id);
            let _ = self.write_entry(&config);
        }
    }

    pub fn remove_favorite(&mut self, id: String, config: &Config) {
        if let Some(pos) = self.favorites.iter().position(|e| e == &id) {
            self.favorites.remove(pos);
            let _ = self.write_entry(&config);
        }
    }
}
