// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    Application,
    applet::{menu_button, token::subscription::TokenRequest},
    cctk::sctk::reexports::calloop,
    iced,
    widget::icon,
};
use std::{
    cell::RefCell,
    path::PathBuf,
};
use tracing::warn;

use crate::subscriptions::status_notifier_item::{IconUpdate, Layout, StatusNotifierItem};

#[derive(Debug, Clone, PartialEq, Eq)]
enum IconLogState {
    NamedResolved {
        name: String,
        path: PathBuf,
    },
    NamedMissing {
        name: String,
    },
    Pixmap {
        width: i32,
        height: i32,
        bytes: usize,
    },
}

#[derive(Clone, Debug)]
pub enum Msg {
    Layout(Result<Layout, String>),
    Icon(IconUpdate),
    Click(i32, bool),
    ClickToken(String),
}

pub struct State {
    pub item: StatusNotifierItem,
    layout: Option<Layout>,
    expanded: Option<i32>,
    icon_name: String,
    // TODO handle icon with multiple sizes?
    icon_pixmap: Option<icon::Handle>,
    click_event: Option<(i32, bool)>,
    icon_log: RefCell<Option<IconLogState>>,
}

impl State {
    pub fn new(item: StatusNotifierItem) -> (Self, iced::Task<Msg>) {
        (
            Self {
                item,
                layout: None,
                expanded: None,
                icon_name: String::new(),
                icon_pixmap: None,
                click_event: None,
                icon_log: RefCell::new(None),
            },
            iced::Task::none(),
        )
    }

    pub fn update(
        &mut self,
        message: Msg,
        menu_id: usize,
        token_tx: Option<&calloop::channel::Sender<TokenRequest>>,
    ) -> iced::Task<Msg> {
        match message {
            Msg::Layout(layout) => {
                match layout {
                    Ok(layout) => {
                        self.layout = Some(layout);
                    }
                    Err(err) => eprintln!("Error getting layout from icon: {err}"),
                }
                iced::Task::none()
            }
            Msg::Icon(update) => {
                match update {
                    IconUpdate::Name(name) => {
                        if self.icon_name != name {
                            self.icon_name = name;
                            self.log_named_icon();
                        }
                    }
                    IconUpdate::Pixmap(icons) => {
                        if icons.is_empty() {
                            self.record_icon_log(
                                IconLogState::Pixmap {
                                    width: 0,
                                    height: 0,
                                    bytes: 0,
                                },
                                || {
                                    warn!(
                                        target: "cosmic_panel::tray::icon",
                                        item = %self.item.name(),
                                        "Status notifier item sent empty icon pixmap list"
                                    );
                                },
                            );
                            self.icon_pixmap = None;
                        } else {
                            self.icon_pixmap = icons
                                .into_iter()
                                .max_by_key(|i| (i.width, i.height))
                                .map(|mut i| {
                                    if i.width <= 0 || i.height <= 0 || i.bytes.is_empty() {
                                        warn!(
                                            target: "cosmic_panel::tray::icon",
                                            item = %self.item.name(),
                                            width = i.width,
                                            height = i.height,
                                            bytes = i.bytes.len(),
                                            "Skipping invalid tray icon pixmap dimensions"
                                        );
                                        return icon::from_name("dialog-question")
                                            .symbolic(true)
                                            .handle();
                                    }
                                    // Convert ARGB to RGBA
                                    for pixel in i.bytes.chunks_exact_mut(4) {
                                        pixel.rotate_left(1);
                                    }

                                    // Do not log success for pixmap updates to reduce noise.
                                    self.record_icon_log(
                                        IconLogState::Pixmap {
                                            width: i.width,
                                            height: i.height,
                                            bytes: i.bytes.len(),
                                        },
                                        || {},
                                    );

                                    icon::from_raster_pixels(
                                        i.width as u32,
                                        i.height as u32,
                                        i.bytes,
                                    )
                                });
                        }
                    }
                }

                iced::Task::none()
            }
            Msg::Click(id, is_submenu) => {
                if let Some(token_tx) = token_tx {
                    _ = token_tx.send(TokenRequest {
                        app_id: super::app::App::APP_ID.to_string(),
                        exec: menu_id.to_string(),
                    });
                }
                self.click_event = Some((id, is_submenu));
                iced::Task::none()
            }
            Msg::ClickToken(token) => {
                let Some((id, is_submenu)) = self.click_event else {
                    return iced::Task::none();
                };
                let item_proxy = self.item.item_proxy().clone();

                let Some(menu_proxy) = self.item.menu_proxy().cloned() else {
                    tokio::spawn(async move {
                        let _ = item_proxy.provide_xdg_activation_token(token).await;
                        if let Err(err) = item_proxy.activate(0, 0).await {
                            tracing::error!(
                                "Error activating status notifier item without menu proxy: {err:?}"
                            );
                        }
                    });
                    return iced::Task::none();
                };
                tokio::spawn(async move {
                    let _ = item_proxy.provide_xdg_activation_token(token).await;
                    let _ = menu_proxy.event(id, "clicked", &0.into(), 0).await;
                });
                if is_submenu {
                    self.expanded = if self.expanded.is_none() {
                        Some(id)
                    } else {
                        None
                    };
                } else {
                    // TODO: Close menu?
                }
                iced::Task::none()
            }
        }
    }

    pub fn name(&self) -> &str {
        self.item.name()
    }

    pub fn icon_name(&self) -> &str {
        &self.icon_name
    }

    pub fn icon_pixmap(&self) -> Option<&icon::Handle> {
        self.icon_pixmap.as_ref()
    }

    pub fn popup_view(&self) -> cosmic::Element<'_, Msg> {
        if let Some(layout) = self.layout.as_ref() {
            layout_view(layout, self.expanded)
        } else {
            iced::widget::text("").into()
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Msg> {
        iced::Subscription::batch([
            self.item.layout_subscription().map(Msg::Layout),
            self.item.icon_subscription().map(Msg::Icon),
        ])
    }

    pub fn opened(&self) {
        let Some(menu_proxy) = self.item.menu_proxy().cloned() else {
            return;
        };
        tokio::spawn(async move {
            let _ = menu_proxy.event(0, "opened", &0i32.into(), 0).await;
            let _ = menu_proxy.about_to_show(0).await;
        });
    }

    pub fn closed(&self) {
        let Some(menu_proxy) = self.item.menu_proxy().cloned() else {
            return;
        };
        tokio::spawn(async move {
            let _ = menu_proxy.event(0, "closed", &0i32.into(), 0).await;
        });
    }

    fn record_icon_log(&self, new_state: IconLogState, log: impl FnOnce()) {
        let mut current = self.icon_log.borrow_mut();
        if current.as_ref() != Some(&new_state) {
            log();
            *current = Some(new_state);
        }
    }

    fn log_named_icon(&self) {
        let name = self.icon_name.trim();
        if name.is_empty() {
            self.record_icon_log(
                IconLogState::NamedMissing {
                    name: String::new(),
                },
                || {
                    warn!(
                        target: "cosmic_panel::tray::icon",
                        item = %self.item.name(),
                        "Status notifier icon name is empty"
                    );
                },
            );
            return;
        }

        // Prefer SVG only for symbolic icons; otherwise allow raster picks.
        let named = if name.ends_with("-symbolic") {
            icon::from_name(name).prefer_svg(true)
        } else {
            icon::from_name(name)
        };
        let path = named.clone().path();

        match path {
            Some(path) => {
                // Success case: record state to avoid repeated warns, but do not log.
                self.record_icon_log(
                    IconLogState::NamedResolved {
                        name: name.to_string(),
                        path: path.clone(),
                    },
                    || {},
                );
            }
            None => {
                self.record_icon_log(
                    IconLogState::NamedMissing {
                        name: name.to_string(),
                    },
                    || {
                        warn!(
                            target: "cosmic_panel::tray::icon",
                            item = %self.item.name(),
                            icon_name = name,
                            "Tray icon name missing from current icon themes"
                        );
                    },
                );
            }
        }
    }
}

fn layout_view(layout: &Layout, expanded: Option<i32>) -> cosmic::Element<'_, Msg> {
    iced::widget::column(layout.children().iter().filter_map(|i| {
        if !i.visible() {
            None
        } else if i.type_() == Some("separator") {
            Some(iced::widget::horizontal_rule(2).into())
        } else if let Some(label) = i.label() {
            // Strip _ when not doubled
            // TODO: interpret as "access key"? And label with underline.
            let mut is_underscore = false;
            let label = label
                .chars()
                .filter(|c| {
                    let prev_is_underscore = is_underscore;
                    is_underscore = !is_underscore && *c == '_';
                    *c != '_' || prev_is_underscore
                })
                .collect::<String>();

            let is_submenu = i.children_display() == Some("submenu");
            let is_expanded = expanded == Some(i.id());

            let text = iced::widget::text(label).width(iced::Length::Fill);

            let mut children: Vec<cosmic::Element<_>> = vec![text.into()];
            if is_submenu {
                let icon = cosmic::widget::icon::from_name(if is_expanded {
                    "go-down-symbolic"
                } else {
                    "go-next-symbolic"
                })
                .size(14)
                .symbolic(true);
                children.push(icon.into());
            }
            if let Some(icon_data) = i.icon_data() {
                let handle = iced::widget::image::Handle::from_bytes(icon_data.to_vec());
                children.insert(0, iced::widget::Image::new(handle).into());
            } else if let Some(icon_name) = i.icon_name() {
                let icon = cosmic::widget::icon::from_name(icon_name)
                    .size(14)
                    .symbolic(true);
                children.insert(0, icon.into());
            }
            if i.toggle_state() == Some(1) {
                let icon = cosmic::widget::icon::from_name("emblem-ok-symbolic")
                    .size(14)
                    .symbolic(true);
                children.push(icon.into());
            }
            let button = row_button(children).on_press(Msg::Click(i.id(), is_submenu));

            if is_submenu && is_expanded {
                Some(
                    iced::widget::column![
                        button,
                        // XXX nested
                        iced::widget::container(layout_view(i, None)).padding(iced::Padding {
                            left: 12.,
                            ..iced::Padding::ZERO
                        })
                    ]
                    .into(),
                )
            } else {
                Some(button.into())
            }
        } else {
            None
        }
    }))
    .into()
}

fn row_button(content: Vec<cosmic::Element<Msg>>) -> cosmic::widget::Button<Msg> {
    menu_button(
        iced::widget::Row::with_children(content)
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .width(iced::Length::Fill),
    )
}
