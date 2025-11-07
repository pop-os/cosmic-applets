// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod localize;
mod wayland;

use cctk::cosmic_protocols::keyboard_layout::v1::client::zcosmic_keyboard_layout_v1::ZcosmicKeyboardLayoutV1;
use cosmic::iced::{Alignment, Length};
use cosmic::{
    app,
    app::Core,
    applet::{self},
    cosmic_config::{self, ConfigSet, CosmicConfigEntry},
    cosmic_theme::Spacing,
    iced::{
        Task,
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        widget::{column, row},
        window::Id,
    },
    iced_futures::Subscription,
    iced_runtime::{Appearance, core::window},
    prelude::*,
    surface, theme,
    widget::{self, horizontal_space, vertical_space},
};
use cosmic_comp_config::CosmicCompConfig;
use xkb_data::KeyboardLayout;

pub const ID: &str = "com.system76.CosmicAppletInputSources";

pub fn run() -> cosmic::iced::Result {
    localize::localize();

    let layouts = match xkb_data::all_keyboard_layouts() {
        Ok(layouts) => layouts,
        Err(why) => {
            tracing::error!("could not get keyboard layouts data: {:?}", why);
            return Ok(());
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

    cosmic::applet::run::<Window>(Flags {
        comp_config,
        comp_config_handler,
        layouts: layouts.layout_list.layout,
    })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ActiveLayout {
    layout: String,
    description: String,
    variant: String,
}

pub struct Window {
    core: Core,
    popup: Option<Id>,
    comp_config: CosmicCompConfig,
    comp_config_handler: Option<cosmic_config::Config>,
    layouts: Vec<KeyboardLayout>,
    active_layouts: Vec<ActiveLayout>,
    layout_list: Vec<String>,
    keyboard_layout: Option<ZcosmicKeyboardLayoutV1>,
    layout_index: usize,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    CompConfig(Box<CosmicCompConfig>),
    SetActiveLayout(usize),
    KeyboardSettings,
    Surface(surface::Action),
    Wayland(wayland::Event),
}

#[derive(Debug)]
pub struct Flags {
    pub comp_config: CosmicCompConfig,
    pub comp_config_handler: Option<cosmic_config::Config>,
    pub layouts: Vec<KeyboardLayout>,
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

    fn init(core: Core, flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        let window = Window {
            comp_config_handler: flags.comp_config_handler,
            layouts: flags.layouts,
            core,
            popup: None,
            comp_config: flags.comp_config,
            active_layouts: Vec::new(),
            layout_list: Vec::new(),
            keyboard_layout: None,
            layout_index: 0,
        };
        (window, Task::none())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::CompConfig(config) => {
                self.comp_config = *config;
                self.active_layouts = self.update_xkb();
            }
            Message::KeyboardSettings => {
                let mut cmd = std::process::Command::new("cosmic-settings");
                cmd.arg("keyboard");
                tokio::spawn(cosmic::process::spawn(cmd));
            }
            Message::SetActiveLayout(pos) => {
                if let Some(keyboard_layout) = &self.keyboard_layout {
                    keyboard_layout.set_group(pos as u32);
                    use cctk::wayland_client::{Connection, Proxy};
                    if let Some(backend) = keyboard_layout.backend().upgrade() {
                        backend.flush();
                    }
                }
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Message::Wayland(event) => match event {
                wayland::Event::LayoutList(layouts) => {
                    self.layout_list = layouts;
                }
                wayland::Event::Layout(index) => {
                    self.layout_index = index;
                }
                wayland::Event::KeyboardLayout(keyboard_layout) => {
                    self.keyboard_layout = Some(keyboard_layout);
                }
            },
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let input_source_text = self.core.applet.text(
            self.layout_list
                .get(self.layout_index)
                .map_or("", |s| s.as_str()),
        );

        cosmic::widget::button::custom(
            row!(
                column!(
                    input_source_text,
                    horizontal_space().width(Length::Fixed(
                        (self.core.applet.suggested_size(true).0
                            + 2 * self.core.applet.suggested_padding(true))
                            as f32
                    ))
                )
                .width(Length::Shrink)
                .height(Length::Shrink)
                .align_x(Alignment::Center),
                vertical_space().height(Length::Fixed(
                    (self.core.applet.suggested_size(true).1
                        + 2 * self.core.applet.suggested_padding(true)) as f32
                ))
            )
            .align_y(Alignment::Center)
            .width(Length::Shrink)
            .height(Length::Shrink),
        )
        .on_press_down(Message::TogglePopup)
        .class(cosmic::theme::Button::AppletIcon)
        .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let mut content_list =
            widget::column::with_capacity(4 + self.layout_list.len()).padding([8, 0]);
        for (id, layout) in self.layout_list.iter().enumerate() {
            let group = widget::column::with_capacity(2).push(widget::text::body(layout));
            //.push(widget::text::body(layout.description.as_str()))
            //.push(widget::text::caption(layout.layout.as_str()));
            content_list = content_list
                .push(applet::menu_button(group).on_press(Message::SetActiveLayout(id)));
        }
        if !self.layout_list.is_empty() {
            content_list = content_list.push(
                applet::padded_control(widget::divider::horizontal::default())
                    .padding([space_xxs, space_s])
                    .apply(Element::from),
            );
        }
        content_list = content_list.push(
            applet::menu_button(widget::text::body(fl!("keyboard-settings")))
                .on_press(Message::KeyboardSettings),
        );

        self.core.applet.popup_container(content_list).into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch([
            self.core
                .watch_config("com.system76.CosmicComp")
                .map(|update| {
                    if !update.errors.is_empty() {
                        tracing::error!(
                            "errors loading config {:?}: {:?}",
                            update.keys,
                            update.errors
                        );
                    }
                    Message::CompConfig(Box::new(update.config))
                }),
            wayland::subscription().map(Message::Wayland),
        ])
    }

    fn style(&self) -> Option<Appearance> {
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

        'outer: for (layout, variant) in layouts.zip(variants) {
            for xkb_layout in &self.layouts {
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
                    continue 'outer;
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
                    continue 'outer;
                }
            }
        }

        active_layouts
    }
}
