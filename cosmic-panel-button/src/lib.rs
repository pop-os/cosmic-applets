// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use config::{CosmicPanelButtonConfig, IndividualConfig, Override};
use cosmic::desktop::fde::{self, DesktopEntry, get_languages_from_env};
use cosmic::{
    Task, app,
    applet::{
        Size,
        cosmic_panel_config::{PanelAnchor, PanelSize},
    },
    iced, surface,
    widget::icon,
};
use cosmic_config::{Config, CosmicConfigEntry};
use std::{env, fs, process::Command};

mod config;

#[derive(Debug, Clone, Default)]
struct Desktop {
    name: String,
    icon: Option<icon::Handle>,
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
    Surface(surface::Action<Msg>),
}

impl cosmic::Application for Button {
    type Message = Msg;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Desktop;
    const APP_ID: &'static str = "com.system76.CosmicPanelButton";

    fn init(core: cosmic::app::Core, desktop: Desktop) -> (Self, app::Task<Msg>) {
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
            Task::none(),
        )
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
            Msg::Press => {
                if let Ok(mut child) = Command::new("sh")
                    .arg("-c")
                    .arg(format!("exec {}", self.desktop.exec))
                    .spawn()
                {
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                }
            }
            Msg::ConfigUpdated(conf) => {
                self.config = conf
                    .configs
                    .get(&self.core.applet.panel_type.to_string())
                    .cloned()
                    .unwrap_or_default();
            }
            Msg::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Surface(a));
            }
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Msg> {
        let applet = &self.core.applet;
        // currently, panel being anchored to the left or right is a hard
        // override for icon, later if text is updated to wrap, we may
        // use Override::Text to override this behavior
        let side_anchored = matches!(applet.anchor, PanelAnchor::Left | PanelAnchor::Right);
        let force_icon = matches!(self.config.force_presentation, Some(Override::Icon));
        let default_icon = self.config.force_presentation.is_none()
            && matches!(
                applet.size,
                Size::PanelSize(PanelSize::S | PanelSize::M | PanelSize::L | PanelSize::XL)
            );

        let content: cosmic::Element<'_, Msg> = match &self.desktop.icon {
            Some(icon) if side_anchored || force_icon || default_icon => applet
                .applet_tooltip::<Msg>(
                    applet
                        .icon_button_from_handle(icon.to_owned())
                        .on_press(Msg::Press),
                    self.desktop.name.clone(),
                    false,
                    Msg::Surface,
                    None,
                )
                .into(),
            _ => applet
                .text_button(applet.text(&self.desktop.name), Msg::Press)
                .into(),
        };

        applet.autosize_window(content).into()
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

pub fn run() -> iced::Result {
    let id = env::args()
        .nth(1)
        .expect("Requires desktop file id as argument.");
    let filename = format!("{id}.desktop");
    let mut desktop = None;
    let locales = get_languages_from_env();

    for mut path in fde::default_paths() {
        path.push(&filename);
        if let Ok(bytes) = fs::read_to_string(&path)
            && let Ok(entry) = DesktopEntry::from_str(&path, &bytes, Some(&locales))
        {
            desktop = Some(Desktop {
                name: entry.name(&locales).map_or_else(
                    || panic!("Desktop file '{filename}' doesn't have `Name`"),
                    |x| x.to_string(),
                ),
                icon: entry.icon().map(|x| icon::from_name(x).handle()),
                exec: entry.exec().map_or_else(
                    || panic!("Desktop file '{filename}' doesn't have `Exec`"),
                    |x| x.to_string(),
                ),
            });
            break;
        }
    }
    let desktop = desktop.unwrap_or_else(|| {
        panic!("Failed to find valid desktop file '{filename}' in search paths")
    });
    cosmic::applet::run::<Button>(desktop)
}
