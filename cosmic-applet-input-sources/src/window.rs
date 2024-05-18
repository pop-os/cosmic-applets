// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::config::{Config, CONFIG_VERSION};
#[allow(unused_imports)]
use crate::fl;
use cosmic::app::Core;
use cosmic::applet::{self};
use cosmic::cosmic_config::{self, ConfigSet};
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
#[allow(unused_imports)]
use cosmic::iced::{alignment, Alignment, Length};
use cosmic::iced::{Command, Limits};
use cosmic::iced_futures::Subscription;
use cosmic::iced_runtime::core::window;
use cosmic::iced_style::application;
use cosmic::prelude::*;
use cosmic::widget;
use cosmic_comp_config::CosmicCompConfig;
use xkb_data::KeyboardLayouts;

pub const ID: &str = "com.system76.CosmicAppletInputSources";

pub struct Window {
    core: Core,
    popup: Option<Id>,
    config: Config,
    #[allow(dead_code)]
    config_handler: Option<cosmic_config::Config>,
    comp_config: CosmicCompConfig,
    comp_config_handler: Option<cosmic_config::Config>,
    layouts: KeyboardLayouts,
    active_layouts: Vec<ActiveLayout>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Config(Config),
    TogglePopup,
    PopupClosed(Id),
    CompConfig(CosmicCompConfig),
    SetActiveLayout(ActiveLayout),
    KeyboardSettings,
}

#[derive(Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: Config,
    pub comp_config: CosmicCompConfig,
    pub comp_config_handler: Option<cosmic_config::Config>,
    pub layouts: KeyboardLayouts,
}

impl cosmic::Application for Window {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Flags;
    type Message = Message;

    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(
        core: Core,
        flags: Self::Flags,
    ) -> (Self, Command<cosmic::app::Message<Self::Message>>) {
        let window = Window {
            comp_config_handler: flags.comp_config_handler,
            layouts: flags.layouts,
            core,
            config: flags.config,
            config_handler: flags.config_handler,
            popup: None,
            comp_config: flags.comp_config,
            active_layouts: Vec::new(),
        };
        (window, Command::none())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::Config(config) => self.config = config,
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings =
                        self.core
                            .applet
                            .get_popup_settings(Id::MAIN, new_id, None, None, None);
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(1.)
                        .max_height(1080.0);
                    get_popup(popup_settings)
                }
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::CompConfig(config) => {
                self.comp_config = config;
                self.active_layouts = self.update_xkb();
            }
            Message::KeyboardSettings => {
                let mut cmd = std::process::Command::new("cosmic-settings");
                cmd.arg("keyboard");
                cosmic::process::spawn(cmd);
            }
            Message::SetActiveLayout(active_layout) => {
                let Some(i) = self
                    .active_layouts
                    .iter()
                    .position(|layout| layout == &active_layout)
                else {
                    return Command::none();
                };

                self.active_layouts.swap(0, i);
                let mut new_layout = String::new();
                let mut new_variant = String::new();

                for layout in &self.active_layouts {
                    new_layout.push_str(&layout.layout);
                    new_layout.push(',');
                    new_variant.push_str(&layout.variant);
                    new_variant.push(',');
                }
                let _excess_comma = new_layout.pop();
                let _excess_comma = new_variant.pop();

                self.comp_config.xkb_config.layout = new_layout;
                self.comp_config.xkb_config.variant = new_variant;
                if let Some(comp_config_handler) = &self.comp_config_handler {
                    if let Err(err) =
                        comp_config_handler.set("xkb_config", &self.comp_config.xkb_config)
                    {
                        tracing::error!("Failed to set config 'xkb_config' {err}");
                    }
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let suggested = self.core.applet.suggested_padding(true);
        widget::button(
            self.core.applet.text(
                self.active_layouts
                    .first()
                    .map_or(String::new(), |l| l.layout.clone()),
            ),
        )
        .style(cosmic::theme::Button::AppletIcon)
        .padding([suggested / 2, suggested])
        .on_press(Message::TogglePopup)
        .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let mut content_list =
            widget::column::with_capacity(4 + self.active_layouts.len()).padding([8, 0]);
        for layout in &self.active_layouts {
            let group = widget::column::with_capacity(2)
                .push(widget::text::body(layout.description.clone()))
                .push(widget::text::caption(layout.layout.clone()));
            content_list = content_list.push(
                applet::menu_button(group).on_press(Message::SetActiveLayout(layout.clone())),
            );
        }
        if !self.active_layouts.is_empty() {
            content_list = content_list.push(
                applet::padded_control(widget::divider::horizontal::default()).apply(Element::from),
            );
        }
        content_list = content_list.push(
            applet::menu_button(widget::text::body(fl!("keyboard-settings")))
                .on_press(Message::KeyboardSettings),
        );

        self.core.applet.popup_container(content_list).into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        struct ConfigSubscription;
        let config = cosmic_config::config_subscription(
            std::any::TypeId::of::<ConfigSubscription>(),
            Self::APP_ID.into(),
            CONFIG_VERSION,
        )
        .map(|update| {
            if !update.errors.is_empty() {
                tracing::error!(
                    "errors loading config {:?}: {:?}",
                    update.keys,
                    update.errors
                );
            }
            Message::Config(update.config)
        });
        let xbg_config = self
            .core
            .watch_config("com.system76.CosmicComp")
            .map(|update| {
                if !update.errors.is_empty() {
                    tracing::error!(
                        "errors loading config {:?}: {:?}",
                        update.keys,
                        update.errors
                    );
                }
                Message::CompConfig(update.config)
            });
        Subscription::batch(vec![config, xbg_config])
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
impl Window {
    fn update_xkb(&self) -> Vec<ActiveLayout> {
        let mut active_layouts = Vec::new();
        let xkb = &self.comp_config.xkb_config;

        let layouts = xkb.layout.split_terminator(',');

        let variants = xkb
            .variant
            .split_terminator(',')
            .chain(std::iter::repeat(""));

        for (layout, variant) in layouts.zip(variants) {
            println!("{} : {}", layout, variant);
            for xkb_layout in self.layouts.layouts() {
                if layout != xkb_layout.name() {
                    continue;
                }
                if variant.is_empty() {
                    let active_layout = ActiveLayout {
                        description: xkb_layout.description().to_owned(),
                        layout: layout.to_owned(),
                        variant: variant.to_owned(),
                    };
                    active_layouts.push(active_layout);

                    continue;
                }

                let Some(xkb_variants) = xkb_layout.variants() else {
                    continue;
                };
                for xkb_variant in xkb_variants {
                    if variant != xkb_variant.name() {
                        continue;
                    }
                    let active_layout = ActiveLayout {
                        description: xkb_variant.description().to_owned(),
                        layout: layout.to_owned(),
                        variant: variant.to_owned(),
                    };
                    active_layouts.push(active_layout);
                }
            }
        }
        active_layouts
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ActiveLayout {
    layout: String,
    description: String,
    variant: String,
}
