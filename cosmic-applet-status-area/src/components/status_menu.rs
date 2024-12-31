// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{applet::menu_button, iced, widget::icon};

use crate::subscriptions::status_notifier_item::{IconNameOrPixmap, Layout, StatusNotifierItem};

#[derive(Clone, Debug)]
pub enum Msg {
    Icon(Option<IconNameOrPixmap>),
    Layout(Result<Layout, String>),
    Tooltip(String),
    Click(i32, bool),
}

#[derive(Debug)]
pub struct State {
    item: StatusNotifierItem,
    layout: Option<Layout>,
    tooltip: String,
    icon: Option<IconNameOrPixmap>,
    expanded: Option<i32>,
}

impl State {
    pub fn new(item: StatusNotifierItem) -> (Self, iced::Task<Msg>) {
        (
            Self {
                item,
                layout: None,
                expanded: None,
                icon: None,
                tooltip: Default::default(),
            },
            iced::Task::none(),
        )
    }

    pub fn update(&mut self, message: Msg) -> iced::Task<Msg> {
        match message {
            Msg::Icon(icon) => {
                self.icon = icon;
                iced::Task::none()
            }
            Msg::Tooltip(tooltip) => {
                self.tooltip = tooltip;
                iced::Task::none()
            }
            Msg::Layout(layout) => {
                match layout {
                    Ok(layout) => {
                        self.layout = Some(layout);
                    }
                    Err(err) => eprintln!("Error getting layout from icon: {}", err),
                }
                iced::Task::none()
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
                iced::Task::none()
            }
        }
    }

    pub fn name(&self) -> &str {
        self.item.name()
    }

    pub fn icon_handle(&self) -> icon::Handle {
        self.icon
            .as_ref()
            .map(|i| i.clone().into())
            .unwrap_or_else(|| icon::from_raster_bytes(&[]))
    }

    pub fn tooltip(&self) -> &str {
        &self.tooltip
    }

    pub fn popup_view(&self) -> cosmic::Element<Msg> {
        if let Some(layout) = self.layout.as_ref() {
            layout_view(layout, self.expanded)
        } else {
            iced::widget::text("").into()
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Msg> {
        let subs = vec![
            self.item.icon_subscription().map(Msg::Icon),
            self.item.tooltip_subscription().map(Msg::Tooltip),
            self.item.layout_subscription().map(Msg::Layout),
        ];

        iced::Subscription::batch(subs)
    }

    pub fn opened(&self) {
        let menu_proxy = self.item.menu_proxy().clone();
        tokio::spawn(async move {
            let _ = menu_proxy.event(0, "opened", &0i32.into(), 0).await;
            let _ = menu_proxy.about_to_show(0).await;
        });
    }

    pub fn closed(&self) {
        let menu_proxy = self.item.menu_proxy().clone();
        tokio::spawn(async move {
            let _ = menu_proxy.event(0, "closed", &0i32.into(), 0).await;
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
