// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    Application,
    applet::{menu_button, token::subscription::TokenRequest},
    cctk::sctk::reexports::calloop,
    iced,
    widget::icon::{self, IconFallback},
};
use std::path::{Path, PathBuf};

use crate::subscriptions::status_notifier_item::{IconUpdate, Layout, StatusNotifierItem};

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
    // TODO handle icon with multiple sizes?
    icon_handle: icon::Handle,
    click_event: Option<(i32, bool)>,
}

impl State {
    pub fn new(item: StatusNotifierItem) -> (Self, iced::Task<Msg>) {
        (
            Self {
                item,
                layout: None,
                expanded: None,
                icon_handle: icon::from_name("application-default")
                    .prefer_svg(true)
                    .handle(),
                click_event: None,
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
                let icon_name = update.name.unwrap_or_default();

                // Use the icon pixmap if an icon was not defined by name.
                if icon_name.is_empty() {
                    let icon_pixmap = update.pixmap.and_then(|icons| icons
                        .into_iter()
                        .max_by_key(|i| (i.width, i.height))
                        .map(|mut i| {
                            if i.width <= 0 || i.height <= 0 || i.bytes.is_empty() {
                                // App sent invalid icon data during initialization - show placeholder until NewIcon signal
                                tracing::debug!("Skipping invalid icon: {}x{} with {} bytes, app may still be initializing",
                                        i.width, i.height, i.bytes.len());
                                return icon::from_name("dialog-question").symbolic(true).handle();
                            }
                            // Convert ARGB to RGBA
                            for pixel in i.bytes.chunks_exact_mut(4) {
                                pixel.rotate_left(1);
                            }
                            icon::from_raster_pixels(i.width as u32, i.height as u32, i.bytes)
                        }));

                    if let Some(icon) = icon_pixmap {
                        self.icon_handle = icon.clone();
                        return iced::Task::none();
                    }
                }

                // Load icon by path if the name is a path.
                self.icon_handle = if Path::new(&icon_name).exists() {
                    icon::from_path(Path::new(&icon_name).to_path_buf()).symbolic(true)
                } else {
                    icon::from_name(icon_name)
                        .prefer_svg(true)
                        .fallback(Some(IconFallback::Names(vec![
                            "application-default".into(),
                            "application-x-executable".into(),
                        ])))
                        .handle()
                };

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

    pub fn icon_handle(&self) -> &icon::Handle {
        &self.icon_handle
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

            let mut button = row_button(children);
            if i.enabled() {
                button = button.on_press(Msg::Click(i.id(), is_submenu));
            }

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
