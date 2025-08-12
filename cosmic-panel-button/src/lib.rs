// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use config::{CosmicPanelButtonConfig, IndividualConfig, Override};
use cosmic::desktop::fde::{self, get_languages_from_env, DesktopEntry};
use cosmic::{
    app,
    applet::{
        cosmic_panel_config::{PanelAnchor, PanelSize},
        Size,
    },
    iced::{self, Length},
    iced_widget::row,
    surface,
    widget::{autosize, vertical_space, Id},
    Task,
};
use cosmic_config::{Config, CosmicConfigEntry};
use std::{env, fs, process::Command, sync::LazyLock};

mod config;

static AUTOSIZE_MAIN_ID: LazyLock<Id> = LazyLock::new(|| Id::new("autosize-main"));

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
    Surface(surface::Action),
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

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Msg) -> app::Task<Msg> {
        match message {
            Msg::Press => {
                let _ = Command::new("sh")
                    .arg("-c")
                    .arg(&self.desktop.exec)
                    .spawn()
                    .unwrap();
            }
            Msg::ConfigUpdated(conf) => {
                self.config = conf
                    .configs
                    .get(&self.core.applet.panel_type.to_string())
                    .cloned()
                    .unwrap_or_default();
            }
            Msg::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<Msg> {
        // currently, panel being anchored to the left or right is a hard
        // override for icon, later if text is updated to wrap, we may
        // use Override::Text to override this behavior
        autosize::autosize(
            if self.desktop.icon.is_some()
                && matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                )
                || matches!(self.config.force_presentation, Some(Override::Icon))
                || matches!(
                    (&self.core.applet.size, &self.config.force_presentation),
                    (
                        Size::PanelSize(PanelSize::S | PanelSize::M | PanelSize::L | PanelSize::XL),
                        None
                    )
                )
            {
                cosmic::Element::from(
                    self.core.applet.applet_tooltip::<Msg>(
                        self.core
                            .applet
                            .icon_button_from_handle(
                                cosmic::widget::icon::from_name(self.desktop.icon.clone().unwrap())
                                    .handle(),
                            )
                            .on_press_down(Msg::Press),
                        self.desktop.name.clone(),
                        false,
                        Msg::Surface,
                        None,
                    ),
                )
            } else {
                let content = row!(
                    self.core.applet.text(&self.desktop.name),
                    vertical_space().height(Length::Fixed(
                        (self.core.applet.suggested_size(true).1
                            + 2 * self.core.applet.suggested_padding(true))
                            as f32
                    ))
                )
                .align_y(iced::Alignment::Center);
                cosmic::widget::button::custom(content)
                    .padding([0, self.core.applet.suggested_padding(true)])
                    .class(cosmic::theme::Button::AppletIcon)
                    .on_press_down(Msg::Press)
                    .into()
            },
            AUTOSIZE_MAIN_ID.clone(),
        )
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

pub fn run() -> iced::Result {
    let id = env::args()
        .nth(1)
        .expect("Requires desktop file id as argument.");
    let filename = format!("{id}.desktop");
    let mut desktop = None;
    let locales = get_languages_from_env();

    for mut path in fde::default_paths() {
        path.push(&filename);
        if let Ok(bytes) = fs::read_to_string(&path) {
            if let Ok(entry) = DesktopEntry::from_str(&path, &bytes, Some(&locales)) {
                desktop = Some(Desktop {
                    name: entry
                        .name(&locales)
                        .map(|x| x.to_string())
                        .unwrap_or_else(|| panic!("Desktop file '{filename}' doesn't have `Name`")),
                    icon: entry.icon().map(|x| x.to_string()),
                    exec: entry
                        .exec()
                        .map(|x| x.to_string())
                        .unwrap_or_else(|| panic!("Desktop file '{filename}' doesn't have `Exec`")),
                });
                break;
            }
        }
    }
    let desktop = desktop.unwrap_or_else(|| {
        panic!("Failed to find valid desktop file '{filename}' in search paths")
    });
    cosmic::applet::run::<Button>(desktop)
}
