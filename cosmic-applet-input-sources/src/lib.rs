// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::window::Window;
use config::{Config, CONFIG_VERSION};
use cosmic::{cosmic_config, cosmic_config::CosmicConfigEntry};
mod config;
use cosmic_comp_config::CosmicCompConfig;
use window::Flags;
mod localize;
mod window;
pub fn run() -> cosmic::iced::Result {
    localize::localize();

    let layouts = match xkb_data::all_keyboard_layouts() {
        Ok(layouts) => layouts,
        Err(why) => {
            tracing::error!("could not get keyboard layouts data: {:?}", why);
            return Ok(());
        }
    };

    let (config_handler, config) = match cosmic_config::Config::new(window::ID, CONFIG_VERSION) {
        Ok(config_handler) => {
            let config = match Config::get_entry(&config_handler) {
                Ok(ok) => ok,
                Err((errs, config)) => {
                    tracing::error!("errors loading config: {:?}", errs);
                    config
                }
            };
            (Some(config_handler), config)
        }
        Err(err) => {
            tracing::error!("failed to create config handler: {}", err);
            (None, Config::default())
        }
    };

    let (comp_config_handler, comp_config) =
        match cosmic_config::Config::new("com.system76.CosmicComp", CosmicCompConfig::VERSION) {
            Ok(config_handler) => {
                let config = match CosmicCompConfig::get_entry(&config_handler) {
                    Ok(ok) => ok,
                    Err((errs, config)) => {
                        tracing::error!("errors loading config: {:?}", errs);
                        config
                    }
                };
                (Some(config_handler), config)
            }
            Err(err) => {
                tracing::error!("failed to create config handler: {}", err);
                (None, CosmicCompConfig::default())
            }
        };

    let flags = Flags {
        comp_config,
        comp_config_handler,
        config_handler,
        config,
        layouts,
    };
    cosmic::applet::run::<Window>(flags)
}
