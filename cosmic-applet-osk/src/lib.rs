// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{Task, app, cosmic_config::CosmicConfigEntry, iced};
use cosmic_osk_config::Config;

struct Button {
    core: cosmic::app::Core,
}

#[derive(Debug, Clone)]
enum Msg {
    Press,
}

impl cosmic::Application for Button {
    type Message = Msg;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletOsk";

    fn init(core: cosmic::app::Core, _: ()) -> (Self, app::Task<Msg>) {
        (Self { core }, Task::none())
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Msg) -> app::Task<Msg> {
        match message {
            Msg::Press => match Config::handler() {
                Ok(handler) => {
                    let mut config = match Config::get_entry(&handler) {
                        Ok(config) => config,
                        Err((errs, config)) => {
                            tracing::warn!("failed to parse OSK config: {:?}", errs);
                            config
                        }
                    };
                    if let Err(err) = config.set_always_shown(&handler, !config.always_shown) {
                        tracing::error!("failed to set OSK always_shown config: {}", err);
                    }
                }
                Err(err) => {
                    tracing::error!("failed to create OSK config handler: {}", err);
                }
            },
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Msg> {
        self.core
            .applet
            .icon_button("input-keyboard-symbolic")
            .on_press_down(Msg::Press)
            .into()
    }
}

pub fn run() -> iced::Result {
    cosmic::applet::run::<Button>(())
}
