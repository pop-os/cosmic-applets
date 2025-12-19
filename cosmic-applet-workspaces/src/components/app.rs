/*
Copyright 2023 System76 <info@system76.com>
SPDX-License-Identifier: GPL-3.0-only
*/

use std::{process::Command as ShellCommand, rc::Rc, sync::LazyLock, time::Duration};

use cctk::{
    sctk::reexports::calloop::channel::SyncSender,
    sctk::reexports::protocols::ext::workspace::v1::client::ext_workspace_handle_v1::{
        self, ExtWorkspaceHandleV1,
    },
    workspace::Workspace,
};

use cosmic::{
    Element, Task, Theme, app,
    applet::{cosmic_panel_config::PanelAnchor, menu_button, padded_control},
    iced::{
        self, Alignment, Background,
        Event::Mouse,
        Length, Limits, Subscription, event,
        mouse::{self, ScrollDelta},
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        widget::{button, column, row},
        window,
    },
    iced_core::Border,
    scroll::DiscreteScrollState,
    theme,
    widget::{
        Id, autosize, container, divider, horizontal_space, icon, mouse_area, svg, text,
        vertical_space,
    },
};

use crate::config;
use crate::wayland::WorkspaceEvent;
use crate::wayland_subscription::{WorkspacesUpdate, workspaces};

static AUTOSIZE_MAIN_ID: LazyLock<Id> = LazyLock::new(|| Id::new("autosize-main"));

const SCROLL_RATE_LIMIT: Duration = Duration::from_millis(200);

pub fn run() -> cosmic::iced::Result {
    crate::localize::localize();
    cosmic::applet::run::<IcedWorkspacesApplet>(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberFormat {
    Western,
    Japanese,
    Roman,
}

impl NumberFormat {
    pub fn to_u8(self) -> u8 {
        match self {
            NumberFormat::Western => 0,
            NumberFormat::Japanese => 1,
            NumberFormat::Roman => 2,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => NumberFormat::Japanese,
            2 => NumberFormat::Roman,
            _ => NumberFormat::Western,
        }
    }
}

struct IcedWorkspacesApplet {
    core: cosmic::app::Core,
    workspaces: Vec<Workspace>,
    workspace_tx: Option<SyncSender<WorkspaceEvent>>,
    layout: Layout,
    scroll: DiscreteScrollState,
    popup: Option<window::Id>,
    number_format: NumberFormat,
}

impl IcedWorkspacesApplet {
    fn popup_index(&self) -> Option<usize> {
        let mut index = None;
        let Some(max_major_axis_len) =
            self.core
                .applet
                .suggested_bounds
                .as_ref()
                .map(|c| match self.core.applet.anchor {
                    PanelAnchor::Top | PanelAnchor::Bottom => c.width as u32,
                    PanelAnchor::Left | PanelAnchor::Right => c.height as u32,
                })
        else {
            return index;
        };

        let button_total_size = self.core.applet.suggested_size(true).0
            + self.core.applet.suggested_padding(true).1 * 2
            + 4;
        let btn_count = max_major_axis_len / button_total_size as u32;
        if btn_count >= self.workspaces.len() as u32 {
            index = None;
        } else {
            index = Some((btn_count as usize).min(self.workspaces.len()));
        }
        index
    }

    fn format_number(&self, n: usize) -> String {
        match self.number_format {
            NumberFormat::Western => n.to_string(),
            NumberFormat::Japanese => {
                let digits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
                let num = n as i32;
                if num == 0 {
                    return digits[0].to_string();
                }
                if num <= 10 {
                    if num == 10 {
                        return "十".to_string();
                    }
                    return digits[num as usize].to_string();
                }
                if num < 20 {
                    let ones = digits[(num % 10) as usize];
                    return format!("十{}", ones);
                }
                if num < 100 {
                    let tens = (num / 10) as usize;
                    let ones = (num % 10) as usize;
                    if ones == 0 {
                        return format!("{}十", digits[tens]);
                    } else {
                        return format!("{}十{}", digits[tens], digits[ones]);
                    }
                }
                num.to_string()
            }
            NumberFormat::Roman => {
                let mut num = n as i32;
                if num <= 0 {
                    return String::new();
                }
                let vals = [
                    (1000, "M"),
                    (900, "CM"),
                    (500, "D"),
                    (400, "CD"),
                    (100, "C"),
                    (90, "XC"),
                    (50, "L"),
                    (40, "XL"),
                    (10, "X"),
                    (9, "IX"),
                    (5, "V"),
                    (4, "IV"),
                    (1, "I"),
                ];
                let mut res = String::new();
                for (v, s) in vals {
                    while num >= v {
                        res.push_str(s);
                        num -= v;
                    }
                }
                if res.is_empty() { n.to_string() } else { res }
            }
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    WorkspaceUpdate(WorkspacesUpdate),
    WorkspacePressed(ExtWorkspaceHandleV1),
    WheelScrolled(ScrollDelta),
    WorkspaceOverview,
    TogglePopup,
    SelectFormat(NumberFormat),
}

impl cosmic::Application for IcedWorkspacesApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        let persisted = config::WorkspacesAppletConfig::current_number_format();
        let fmt = NumberFormat::from_u8(persisted);

        (
            Self {
                layout: match core.applet.anchor {
                    PanelAnchor::Left | PanelAnchor::Right => Layout::Column,
                    PanelAnchor::Top | PanelAnchor::Bottom => Layout::Row,
                },
                core,
                workspaces: Vec::new(),
                workspace_tx: None,
                scroll: DiscreteScrollState::default().rate_limit(Some(SCROLL_RATE_LIMIT)),
                popup: None,
                number_format: fmt,
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

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::WorkspaceUpdate(msg) => match msg {
                WorkspacesUpdate::Workspaces(mut list) => {
                    list.retain(|w| !w.state.contains(ext_workspace_handle_v1::State::Hidden));
                    list.sort_by(|a, b| a.coordinates.cmp(&b.coordinates));
                    self.workspaces = list;
                }
                WorkspacesUpdate::Started(tx) => {
                    self.workspace_tx.replace(tx);
                }
                WorkspacesUpdate::Errored => {}
            },
            Message::WorkspacePressed(handle) => {
                if let Some(tx) = self.workspace_tx.as_mut() {
                    let _ = tx.try_send(WorkspaceEvent::Activate(handle));
                }
            }
            Message::WheelScrolled(delta) => {
                let discrete_delta = self.scroll.update(delta);
                if discrete_delta.y != 0
                    && let Some(active_idx) = self
                        .workspaces
                        .iter()
                        .position(|w| w.state.contains(ext_workspace_handle_v1::State::Active))
                {
                    let d_i = (active_idx as isize - discrete_delta.y)
                        .rem_euclid(self.workspaces.len() as isize)
                        as usize;
                    if let Some(tx) = self.workspace_tx.as_mut() {
                        let _ = tx.try_send(WorkspaceEvent::Activate(
                            self.workspaces[d_i].handle.clone(),
                        ));
                    }
                }
            }
            Message::WorkspaceOverview => {
                let _ = ShellCommand::new("cosmic-workspaces").spawn();
            }
            Message::TogglePopup => {
                if let Some(id) = self.popup.take() {
                    return destroy_popup(id);
                } else {
                    let new_id = window::Id::unique();
                    self.popup = Some(new_id);
                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    return get_popup(popup_settings);
                }
            }
            Message::SelectFormat(fmt) => {
                self.number_format = fmt;
                let _ = config::WorkspacesAppletConfig::write_number_format(fmt.to_u8());
                if let Some(id) = self.popup.take() {
                    return destroy_popup(id);
                }
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        if self.workspaces.is_empty() {
            return row(vec![]).padding(8).into();
        }

        let horizontal = matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        );
        let suggested_total = self.core.applet.suggested_size(true).0
            + self.core.applet.suggested_padding(true).1 * 2;
        let suggested_window_size = self.core.applet.suggested_window_size();
        let popup_index = self.popup_index().unwrap_or(self.workspaces.len());

        let mut children: Vec<Element<'_, Message>> = Vec::new();

        for (i, w) in self.workspaces[..popup_index].iter().enumerate() {
            let number = self.format_number(i + 1);
            let label = if !w.name.is_empty() {
                if w.name.chars().all(|c| c.is_ascii_digit()) {
                    number.clone()
                } else {
                    format!("{} {}", number, w.name)
                }
            } else {
                number.clone()
            };

            let txt = self.core.applet.text(label).font(cosmic::font::bold());

            let (width, height) = if self.core.applet.is_horizontal() {
                (suggested_total as f32, suggested_window_size.1.get() as f32)
            } else {
                (suggested_window_size.0.get() as f32, suggested_total as f32)
            };

            let content = row(vec![
                txt.into(),
                vertical_space().height(Length::Fixed(height)).into(),
            ])
            .align_y(iced::Alignment::Center);

            let content = column(vec![
                content.into(),
                horizontal_space().width(Length::Fixed(width)).into(),
            ])
            .align_x(iced::Alignment::Center);

            let btn = button(
                container(content)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .padding(if horizontal {
                [0, self.core.applet.suggested_padding(true).1]
            } else {
                [self.core.applet.suggested_padding(true).1, 0]
            })
            .on_press(
                if w.state.contains(ext_workspace_handle_v1::State::Active) {
                    Message::WorkspaceOverview
                } else {
                    Message::WorkspacePressed(w.handle.clone())
                },
            )
            .padding(0);

            let btn = if w.state.contains(ext_workspace_handle_v1::State::Active) {
                btn.class(cosmic::theme::iced::Button::Primary)
            } else if w.state.contains(ext_workspace_handle_v1::State::Urgent) {
                let appearance = |theme: &Theme| {
                    let cosmic = theme.cosmic();
                    button::Style {
                        background: Some(Background::Color(cosmic.palette.neutral_3.into())),
                        border: Border {
                            radius: cosmic.radius_xl().into(),
                            ..Default::default()
                        },
                        text_color: theme.cosmic().destructive_button.base.into(),
                        ..button::Style::default()
                    }
                };
                btn.class(cosmic::theme::iced::Button::Custom(Box::new(
                    move |theme, status| match status {
                        button::Status::Active => appearance(theme),
                        button::Status::Hovered => button::Style {
                            background: Some(Background::Color(
                                theme.current_container().component.hover.into(),
                            )),
                            border: Border {
                                radius: theme.cosmic().radius_xl().into(),
                                ..Default::default()
                            },
                            ..appearance(theme)
                        },
                        button::Status::Pressed => appearance(theme),
                        button::Status::Disabled => appearance(theme),
                    },
                )))
            } else {
                let appearance = |theme: &Theme| {
                    let cosmic = theme.cosmic();
                    button::Style {
                        background: None,
                        border: Border {
                            radius: cosmic.radius_xl().into(),
                            ..Default::default()
                        },
                        text_color: theme.current_container().component.on.into(),
                        ..button::Style::default()
                    }
                };
                btn.class(cosmic::theme::iced::Button::Custom(Box::new(
                    move |theme, status| match status {
                        button::Status::Active => appearance(theme),
                        button::Status::Hovered => button::Style {
                            background: Some(Background::Color(
                                theme.current_container().component.hover.into(),
                            )),
                            border: Border {
                                radius: theme.cosmic().radius_xl().into(),
                                ..Default::default()
                            },
                            ..appearance(theme)
                        },
                        button::Status::Pressed | button::Status::Disabled => appearance(theme),
                    },
                )))
            };

            children.push(btn.into());
        }

        let layout_section: Element<'_, Message> = {
            match self.layout {
                Layout::Row => row(children).spacing(4).into(),
                Layout::Column => column(children).spacing(4).into(),
            }
        };

        let mut limits = Limits::NONE.min_width(1.).min_height(1.);
        if let Some(b) = self.core.applet.suggested_bounds {
            if b.width as i32 > 0 {
                limits = limits.max_width(b.width);
            }
            if b.height as i32 > 0 {
                limits = limits.max_height(b.height);
            }
        }

        autosize::autosize(
            container(mouse_area(layout_section).on_right_release(Message::TogglePopup)).padding(0),
            AUTOSIZE_MAIN_ID.clone(),
        )
        .limits(limits)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            workspaces().map(Message::WorkspaceUpdate),
            event::listen_with(|e, _, _| match e {
                Mouse(mouse::Event::WheelScrolled { delta }) => Some(Message::WheelScrolled(delta)),
                _ => None,
            }),
        ])
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if Some(id) != self.popup {
            return text::body("").into();
        }

        let svg_accent = Rc::new(|theme: &cosmic::Theme| {
            let color = theme.cosmic().accent_color().into();
            svg::Style { color: Some(color) }
        });

        let spacing = theme::active().cosmic().spacing;
        let space_xxs = spacing.space_xxs;
        let space_s = spacing.space_s;

        let mut content = column(vec![]).padding([8, 0]).spacing(0);

        content = content.push(
            padded_control(
                text::body(crate::fl!("number-format-title"))
                    .font(cosmic::font::bold())
                    .width(Length::Fill),
            )
            .padding(cosmic::applet::menu_control_padding()),
        );
        content = content
            .push(padded_control(divider::horizontal::default()).padding([space_xxs, space_s]));

        content = content.push(
            menu_button(
                row(vec![
                    text::body(crate::fl!("western-format"))
                        .width(Length::Fill)
                        .into(),
                    Element::from(
                        container(if self.number_format == NumberFormat::Western {
                            Element::from(
                                icon::icon(icon::from_name("checkbox-checked-symbolic").into())
                                    .size(16)
                                    .class(cosmic::theme::Svg::Custom(svg_accent.clone())),
                            )
                        } else {
                            Element::from(horizontal_space().width(Length::Fixed(16.0)))
                        })
                        .center(Length::Fixed(24.0)),
                    ),
                ])
                .spacing(12)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::SelectFormat(NumberFormat::Western))
            .width(Length::Fill),
        );

        content = content.push(
            menu_button(
                row(vec![
                    text::body(crate::fl!("japanese-format"))
                        .width(Length::Fill)
                        .into(),
                    Element::from(
                        container(if self.number_format == NumberFormat::Japanese {
                            Element::from(
                                icon::icon(icon::from_name("checkbox-checked-symbolic").into())
                                    .size(16)
                                    .class(cosmic::theme::Svg::Custom(svg_accent.clone())),
                            )
                        } else {
                            Element::from(horizontal_space().width(Length::Fixed(16.0)))
                        })
                        .center(Length::Fixed(24.0)),
                    ),
                ])
                .spacing(12)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::SelectFormat(NumberFormat::Japanese))
            .width(Length::Fill),
        );

        content = content.push(
            menu_button(
                row(vec![
                    text::body(crate::fl!("roman-format"))
                        .width(Length::Fill)
                        .into(),
                    Element::from(
                        container(if self.number_format == NumberFormat::Roman {
                            Element::from(
                                icon::icon(icon::from_name("checkbox-checked-symbolic").into())
                                    .size(16)
                                    .class(cosmic::theme::Svg::Custom(svg_accent.clone())),
                            )
                        } else {
                            Element::from(horizontal_space().width(Length::Fixed(16.0)))
                        })
                        .center(Length::Fixed(24.0)),
                    ),
                ])
                .spacing(12)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::SelectFormat(NumberFormat::Roman))
            .width(Length::Fill),
        );

        self.core.applet.popup_container(content).into()
    }
}
