use anyhow::anyhow;
use serde::Deserialize;
use std::fmt::Debug;
use std::fs::File;
use xdg::BaseDirectories;

pub const APP_ID: &str = "com.system76.CosmicAppList";
pub const VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Deserialize, Default)]
pub enum TopLevelFilter {
    #[default]
    ActiveWorkspace,
    ConfiguredOutput,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AppListConfig {
    pub filter_top_levels: Option<TopLevelFilter>,
    pub favorites: Vec<String>,
}

impl AppListConfig {
    /// load config with the provided name
    pub fn load() -> anyhow::Result<AppListConfig> {
        let file = match BaseDirectories::new()
            .ok()
            .and_then(|dirs| dirs.find_config_file(format!("{APP_ID}/config.ron")))
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

    pub fn add_favorite(&mut self, id: String) -> anyhow::Result<()> {
        if !self.favorites.contains(&id) {
            self.favorites.push(id);
        }
        todo!()
    }

    pub fn remove_favorite(&mut self, id: String) -> anyhow::Result<()> {
        self.favorites.retain(|e| e != &id);
        todo!()
    }

    pub fn save() -> anyhow::Result<()> {
        todo!()
    }
}
