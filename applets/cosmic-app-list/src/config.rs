use anyhow::anyhow;
use serde::{Serialize, Deserialize};
use std::fmt::Debug;
use std::fs::File;
use std::path::PathBuf;
use xdg::BaseDirectories;

pub const APP_ID: &str = "com.system76.CosmicAppList";
pub const VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub enum TopLevelFilter {
    #[default]
    ActiveWorkspace,
    ConfiguredOutput,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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

    pub fn add_favorite(&mut self, id: String) -> anyhow::Result<()> {
        if !self.favorites.contains(&id) {
            self.favorites.push(id);
        }
        self.save()
    }

    pub fn remove_favorite(&mut self, id: String) -> anyhow::Result<()> {
        self.favorites.retain(|e| e != &id);
        self.save()
    }

    // TODO async?
    pub fn save(&self) -> anyhow::Result<()> {
        let bd = BaseDirectories::new()?;
        let mut relative_path = PathBuf::from(APP_ID);
        relative_path.push("config.ron");
        let config_path = bd.place_config_file(relative_path)?;
        let f = File::create(config_path)?;
        ron::ser::to_writer_pretty(f, self, Default::default())?;
        Ok(())
    }
}
