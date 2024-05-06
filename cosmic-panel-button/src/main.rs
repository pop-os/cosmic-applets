// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use config::{CosmicPanelButtonConfig, IndividualConfig, Override};
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::iced::Length;
use cosmic::iced_widget::{row, text};
use cosmic::widget::vertical_space;
use cosmic::{app, iced, iced_style::application, theme::Theme};
use cosmic_config::{Config, CosmicConfigEntry};
use freedesktop_desktop_entry::DesktopEntry;
use std::{env, fs, process::Command};

mod config;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Default)]
struct Desktop {
    name: String,
    icon: Option<String>,
    exec: String,
}

struct Button {
    core: cosmic::app::Core,
    desktop: Desktop,
    config: IndividualConfig,
}

#[derive(Debug, Clone)]
enum Msg {
    Press,
    ConfigUpdated(CosmicPanelButtonConfig),
}

impl cosmic::Application for Button {
    type Message = Msg;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Desktop;
    const APP_ID: &'static str = "com.system76.CosmicPanelButton";

    fn init(core: cosmic::app::Core, desktop: Desktop) -> (Self, app::Command<Msg>) {
        let config = Config::new(Self::APP_ID, CosmicPanelButtonConfig::VERSION)
            .ok()
            .and_then(|c| CosmicPanelButtonConfig::get_entry(&c).ok())
            .unwrap_or_default()
            .configs
            .get(&core.applet.panel_type.to_string())
            .cloned()
            .unwrap_or_default();
        (
            Self {
                core,
                desktop,
                config,
            },
            app::Command::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Msg) -> app::Command<Msg> {
        match message {
            Msg::Press => {
                let _ = Command::new("sh").arg("-c").arg(&self.desktop.exec).spawn();
            }
            Msg::ConfigUpdated(conf) => {
                self.config = conf
                    .configs
                    .get(&self.core.applet.panel_type.to_string())
                    .cloned()
                    .unwrap_or_default();
            }
        }
        app::Command::none()
    }

    fn view(&self) -> cosmic::Element<Msg> {
        // currently, panel being anchored to the left or right is a hard
        // override for icon, later if text is updated to wrap, we may
        // use Override::Text to override this behavior
        if self.desktop.icon.is_some()
            && matches!(
                self.core.applet.anchor,
                PanelAnchor::Left | PanelAnchor::Right
            )
            || matches!(self.config.force_presentation, Some(Override::Icon))
        {
            self.core.applet.icon_button_from_handle(
                cosmic::widget::icon::from_name(self.desktop.icon.clone().unwrap()).handle(),
            )
        } else {
            let content = row!(
                text(&self.desktop.name).size(14.0),
                vertical_space(Length::Fixed(
                    (self.core.applet.suggested_size(true).1
                        + 2 * self.core.applet.suggested_padding(true)) as f32
                ))
            )
            .align_items(iced::Alignment::Center);
            cosmic::widget::button(content)
                .padding([0, self.core.applet.suggested_padding(false)])
                .style(cosmic::theme::Button::AppletIcon)
        }
        .on_press(Msg::Press)
        .into()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        self.core.watch_config(Self::APP_ID).map(|u| {
            for why in u.errors {
                tracing::error!(why = why.to_string(), "Error watching config");
            }
            Msg::ConfigUpdated(u.config)
        })
    }
}

pub fn main() -> iced::Result {
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();
    tracing::info!("Starting panel button applet with version {VERSION}");

    let id = env::args()
        .nth(1)
        .expect("Requires desktop file id as argument.");
    let filename = format!("{id}.desktop");
    let mut desktop = None;
    for mut path in freedesktop_desktop_entry::default_paths() {
        path.push(&filename);
        if let Ok(bytes) = fs::read_to_string(&path) {
            if let Ok(entry) = DesktopEntry::decode(&path, &bytes) {
                desktop =
                    Some(Desktop {
                        name: entry.name(None).map(|x| x.to_string()).unwrap_or_else(|| {
                            panic!("Desktop file '{filename}' doesn't have `Name`")
                        }),
                        icon: entry.icon().map(|x| x.to_string()),
                        exec: entry.exec().map(|x| x.to_string()).unwrap_or_else(|| {
                            panic!("Desktop file '{filename}' doesn't have `Exec`")
                        }),
                    });
                break;
            }
        }
    }
    let desktop = desktop.unwrap_or_else(|| {
        panic!("Failed to find valid desktop file '{filename}' in search paths")
    });
    cosmic::applet::run::<Button>(true, desktop)
}
