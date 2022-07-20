use crate::ID;
use anyhow::anyhow;
use serde::Deserialize;
use std::fmt::Debug;
use std::fs::File;
use xdg::BaseDirectories;

#[derive(Debug, Clone, Deserialize)]
pub enum TopLevelFilter {
    ActiveWorkspace,
    ConfiguredOutput,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AppListConfig {
    pub filter_top_levels: Option<TopLevelFilter>,
}

impl AppListConfig {
    /// load config with the provided name
    pub fn load() -> anyhow::Result<AppListConfig> {
        let file = match BaseDirectories::new()
            .ok()
            .and_then(|dirs| dirs.find_config_file(format!("{ID}/config.ron")))
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
}
