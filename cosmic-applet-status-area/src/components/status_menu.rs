// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::applet::menu_button;
use cosmic::{iced, widget::icon};

use crate::subscriptions::status_notifier_item::{Layout, StatusNotifierItem};

#[derive(Clone, Debug)]
pub enum Msg {
    Layout(Result<Layout, String>),
    Click(i32, bool),
}

pub struct State {
    item: StatusNotifierItem,
    layout: Option<Layout>,
    expanded: Option<i32>,
}

impl State {
    pub fn new(item: StatusNotifierItem) -> (Self, iced::Command<Msg>) {
        (
            Self {
                item,
                layout: None,
                expanded: None,
            },
            iced::Command::none(),
        )
    }

    pub fn update(&mut self, message: Msg) -> iced::Command<Msg> {
        match message {
            Msg::Layout(layout) => {
                match layout {
                    Ok(layout) => {
                        self.layout = Some(layout);
                    }
                    Err(err) => eprintln!("Error getting layout from icon: {}", err),
                }
                iced::Command::none()
            }
            Msg::Click(id, is_submenu) => {
                let menu_proxy = self.item.menu_proxy().clone();
                tokio::spawn(async move {
                    let _ = menu_proxy.event(id, "clicked", &0.into(), 0).await;
                });
                if is_submenu {
                    self.expanded = if self.expanded != Some(id) {
                        Some(id)
                    } else {
                        None
                    };
                } else {
                    // TODO: Close menu?
                }
                iced::Command::none()
            }
        }
    }

    pub fn name(&self) -> &str {
        self.item.name()
    }

    pub fn icon_name(&self) -> &str {
        self.item.icon_name()
    }

    pub fn icon_pixmap(&self) -> Option<&icon::Handle> {
        self.item.icon_pixmap()
    }

    pub fn popup_view(&self) -> cosmic::Element<Msg> {
        if let Some(layout) = self.layout.as_ref() {
            layout_view(layout, self.expanded)
        } else {
            iced::widget::text("").into()
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Msg> {
        self.item.layout_subscription().map(Msg::Layout)
    }

    pub fn about_to_show(&self) {
        let menu_proxy = self.item.menu_proxy().clone();
        tokio::spawn(async move {
            let _ = menu_proxy.about_to_show(0).await;
        });
    }
}

fn layout_view(layout: &Layout, expanded: Option<i32>) -> cosmic::Element<Msg> {
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
                let handle = iced::widget::image::Handle::from_memory(icon_data.to_vec());
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
            .align_items(iced::Alignment::Center)
            .width(iced::Length::Fill),
    )
}
