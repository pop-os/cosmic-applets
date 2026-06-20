// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

mod localize;

use cctk::{
    cosmic_protocols::keyboard_layout::v1::client::zcosmic_keyboard_layout_v1::ZcosmicKeyboardLayoutV1,
    wayland_client::{Connection, Proxy, backend::Backend},
};
use cosmic::{
    app,
    app::Core,
    applet::{self},
    cosmic_config::{self, CosmicConfigEntry},
    cosmic_theme::Spacing,
    iced::Subscription,
    iced::core::window,
    iced::{
        self, Rectangle, Task,
        event::wayland::{Event as WaylandEvent, OutputEvent},
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        window::Id,
    },
    prelude::*,
    surface, theme,
    widget::{
        self, autosize,
        rectangle_tracker::{RectangleTracker, RectangleUpdate, rectangle_tracker_subscription},
    },
};
use cosmic_comp_config::CosmicCompConfig;
use std::{
    os::unix::{
        io::{FromRawFd, RawFd},
        net::UnixStream,
    },
    sync::LazyLock,
};
use xkb_data::KeyboardLayout;

mod wayland;

static AUTOSIZE_MAIN_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("autosize-main"));
pub const ID: &str = "com.system76.CosmicAppletInputSources";

pub fn run() -> cosmic::iced::Result {
    let socket = std::env::var("X_PRIVILEGED_WAYLAND_SOCKET")
        .ok()
        .and_then(|fd| {
            fd.parse::<RawFd>()
                .ok()
                .map(|fd| unsafe { UnixStream::from_raw_fd(fd) })
        });
    let wayland_connection = if let Some(socket) = socket {
        Some(Connection::from_socket(socket).unwrap())
    } else {
        None
    };

    localize::localize();

    let layouts = match xkb_data::all_keyboard_layouts() {
        Ok(layouts) => layouts,
        Err(why) => {
            tracing::error!("could not get keyboard layouts data: {:?}", why);
            return Ok(());
        }
    };

    let comp_config =
        match cosmic_config::Config::new("com.system76.CosmicComp", CosmicCompConfig::VERSION) {
            Ok(config_handler) => {
                let config = match CosmicCompConfig::get_entry(&config_handler) {
                    Ok(ok) => ok,
                    Err((errs, config)) => {
                        tracing::error!("errors loading config: {:?}", errs);
                        config
                    }
                };
                config
            }
            Err(err) => {
                tracing::error!("failed to create config handler: {}", err);
                CosmicCompConfig::default()
            }
        };

    cosmic::applet::run::<Window>(Flags {
        wayland_connection,
        comp_config,
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
    layouts: Vec<KeyboardLayout>,
    active_layouts: Vec<ActiveLayout>,
    rectangle_tracker: Option<RectangleTracker<u32>>,
    rectangle: Rectangle,
    wayland_connection: Option<Connection>,
    keyboard_layout: Option<ZcosmicKeyboardLayoutV1>,
    current_layout: usize,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    CompConfig(Box<CosmicCompConfig>),
    SetActiveLayout(usize),
    KeyboardSettings,
    Surface(surface::Action),
    Rectangle(RectangleUpdate<u32>),
    WaylandConnection(Backend),
    Wayland(wayland::Event),
}

#[derive(Debug)]
pub struct Flags {
    wayland_connection: Option<Connection>,
    pub comp_config: CosmicCompConfig,
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
            layouts: flags.layouts,
            core,
            popup: None,
            comp_config: flags.comp_config,
            active_layouts: Vec::new(),
            rectangle_tracker: None,
            rectangle: Rectangle::default(),
            wayland_connection: flags.wayland_connection,
            keyboard_layout: None,
            current_layout: 0,
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
                    if let Some(backend) = keyboard_layout.backend().upgrade() {
                        let _ = backend.flush();
                    }
                }
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Message::Rectangle(u) => match u {
                RectangleUpdate::Rectangle(r) => {
                    self.rectangle = r.1;
                }
                RectangleUpdate::Init(tracker) => {
                    self.rectangle_tracker = Some(tracker);
                }
            },
            Message::WaylandConnection(backend) => {
                if self.wayland_connection.is_none() {
                    self.wayland_connection = Some(Connection::from_backend(backend));
                }
            }
            Message::Wayland(w) => match w {
                wayland::Event::KeyboardLayout(keyboard_layout) => {
                    self.keyboard_layout = Some(keyboard_layout);
                }
                wayland::Event::Group(group) => {
                    self.current_layout = group as usize;
                }
            },
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let applet_text = if let Some(l) = self.active_layouts.get(self.current_layout) {
            if !l.variant.is_empty() {
                format!("{} ({})", l.layout, l.variant)
            } else {
                l.layout.clone()
            }
        } else {
            String::new()
        };
        let input_source_text = self.core.applet.text(applet_text);
        let button = self
            .core
            .applet
            .text_button(input_source_text, Message::TogglePopup);
        autosize::autosize(
            if let Some(tracker) = self.rectangle_tracker.as_ref() {
                Element::from(tracker.container(0, button).ignore_bounds(true))
            } else {
                button.into()
            },
            AUTOSIZE_MAIN_ID.clone(),
        )
        .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let mut content_list =
            widget::column::with_capacity(4 + self.active_layouts.len()).padding([8, 0]);
        for (id, layout) in self.active_layouts.iter().enumerate() {
            let font = if id == self.current_layout {
                cosmic::font::bold()
            } else {
                cosmic::font::default()
            };
            let group = widget::column::with_capacity(2)
                .push(widget::text::body(layout.description.as_str()).font(font))
                .push(widget::text::caption(layout.layout.as_str()));
            content_list = content_list
                .push(applet::menu_button(group).on_press(Message::SetActiveLayout(id)));
        }
        if !self.active_layouts.is_empty() {
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
        let mut subscriptions = vec![
            rectangle_tracker_subscription(0).map(|e| Message::Rectangle(e.1)),
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
        ];
        subscriptions.push(if let Some(connection) = &self.wayland_connection {
            wayland::subscription(connection.clone()).map(Message::Wayland)
        } else {
            iced::event::listen_with(|evt, _, _| match evt {
                iced::Event::PlatformSpecific(iced::event::PlatformSpecific::Wayland(evt)) => {
                    if let WaylandEvent::Output(OutputEvent::Created(_), output) = evt {
                        if let Some(backend) = output.backend().upgrade() {
                            return Some(Message::WaylandConnection(backend));
                        }
                    }
                    None
                }
                _ => None,
            })
        });
        Subscription::batch(subscriptions)
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
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
            println!("{layout} : {variant}");
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
