// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    Element, Task, app,
    cosmic_theme::Spacing,
    iced::{
        Alignment, Length,
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        widget::{self, column, row},
        window,
    },
    surface, theme,
    widget::{icon, text},
};
use std::{fs, path::PathBuf, sync::LazyLock};

use crate::localize::localize;

pub mod localize;

static SUBSURFACE_ID: LazyLock<cosmic::widget::Id> =
    LazyLock::new(|| cosmic::widget::Id::new("subsurface"));

pub fn run() -> cosmic::iced::Result {
    localize();

    cosmic::applet::run::<Theme>(())
}

struct Theme {
    core: cosmic::app::Core,
    icon_name: String,
    popup: Option<window::Id>,
    subsurface_id: window::Id,
    is_dark: bool,
    theme_file: PathBuf,
}

#[derive(Debug, Clone)]
enum ThemeAction {
    ToggleTheme,
}

#[derive(Debug, Clone)]
enum Message {
    Action(ThemeAction),
    TogglePopup,
    Closed(window::Id),
    Surface(surface::Action),
}

impl cosmic::Application for Theme {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.system76.CosmicAppletTheme";

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        let theme_file = dirs::config_dir()
            .expect("Failed to find config dir")
            .join("cosmic/com.system76.CosmicTheme.Mode/v1/is_dark");

        let is_dark = fs::read_to_string(&theme_file)
            .map(|s| s.trim() == "true")
            .unwrap_or(true); // Default to dark mode

        // Ensure file exists
        if !theme_file.exists() {
            if let Some(parent) = theme_file.parent() {
                fs::create_dir_all(parent).expect("Failed to create theme config directory");
            }
            fs::write(&theme_file, is_dark.to_string()).expect("Failed to write initial theme state");
        }

        let icon_name = if is_dark {
            "weather-clear-night-symbolic"
        } else {
            "weather-sunny-symbolic"
        }.to_string();

        (
            Self {
                core,
                icon_name,
                subsurface_id: window::Id::unique(),
                popup: Option::default(),
                is_dark,
                theme_file,
            },
            Task::none(),
        )
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::Closed(id))
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);

                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    get_popup(popup_settings)
                }
            }
            Message::Action(action) => {
                match action {
                    ThemeAction::ToggleTheme => {
                        self.is_dark = !self.is_dark;
                        fs::write(&self.theme_file, self.is_dark.to_string()).expect("Failed to write theme state");
                        self.icon_name = if self.is_dark {
                            "weather-clear-night-symbolic"
                        } else {
                            "weather-sunny-symbolic"
                        }.to_string();
                        Task::none()
                    }
                }
            }

            Message::Closed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
                Task::none()
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button(&self.icon_name)
            .on_press_down(Message::Action(ThemeAction::ToggleTheme))
            .into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        let Spacing {
            space_xxs,
            space_s,
            space_m,
            ..
        } = theme::active().cosmic().spacing;

        if matches!(self.popup, Some(p) if p == id) {
            let toggle = cosmic::widget::button::custom(
                widget::container(
                    row![
                        icon::from_name(if self.is_dark { "weather-sunny-symbolic" } else { "weather-clear-night-symbolic" }).size(24).symbolic(true).icon(),
                        text::body(fl!("toggle-theme")),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(space_xxs)
                )
                .center(Length::Fill),
            )
            .on_press(Message::Action(ThemeAction::ToggleTheme))
            .height(Length::Fixed(40.0))
            .class(theme::Button::Text);

            let content = column![toggle]
                .align_x(Alignment::Start)
                .padding([8, 0]);

            self.core.applet.popup_container(content).into()
        } else {
            widget::text("").into()
        }
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}


