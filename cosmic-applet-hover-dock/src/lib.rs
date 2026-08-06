// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

//! An applet that shows the pinned applications with hover magnification.
//!
//! It reads the same favourites `cosmic-app-list` does, so the two show the
//! same applications in the same order and either can be put in the dock.
//!
//! # Why the surface is a fixed size
//!
//! cosmic-panel sizes its bar to the thickest applet in it and reserves that
//! much of the screen. So the container is exactly one applet unit thick — the
//! same as every other applet — and the magnification is whatever fits in it.
//! Asking for more would push every window on the display down and leave this
//! row sitting below the buttons beside it.
//!
//! Along the bar it is fixed at the longest the row can ever be, for a
//! different reason: a surface that grew during the animation would make the
//! panel re-lay out the whole bar on every frame. Icons are sized inside the
//! fixed container instead, so `autosize` reports one size throughout.
//!
//! The row is laid out along one axis and only becomes horizontal or vertical
//! where it is turned into widgets, so a panel on any of the four edges shares
//! one implementation.

mod layout;
mod localize;
mod wayland_handler;
mod wayland_subscription;

use std::{borrow::Cow, time::Instant};

use cosmic::cosmic_config::{Config, CosmicConfigEntry};
use cosmic::{
    Element, Task, app,
    applet::cosmic_panel_config::PanelAnchor,
    cctk::{
        sctk::reexports::calloop, toplevel_info::ToplevelInfo,
        wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    },
    desktop::{self, IconSourceExt, fde},
    iced::{
        self, Alignment, Background, Border, Length, Limits, Point, Subscription,
        id::Id as WidgetId,
        platform_specific::shell::wayland::commands::popup::destroy_popup,
        widget::{Column, Row, stack},
        window,
    },
    theme::{self, Button, Container},
    widget::{
        autosize::autosize, button, container, divider, mouse_area,
        space::horizontal as horizontal_space, text,
    },
};
use cosmic_app_list_config::{APP_ID as APP_LIST_ID, AppListConfig};
use std::sync::LazyLock;

use layout::{Metrics, Placed, Spring};
use wayland_subscription::{ToplevelUpdate, WaylandRequest, WaylandUpdate};

static AUTOSIZE_MAIN_ID: LazyLock<WidgetId> = LazyLock::new(|| WidgetId::new("autosize-main"));

/// How much the icon under the pointer grows.
const MAGNIFICATION: f32 = 1.6;
/// How far the pointer's influence reaches, counted in icon widths.
///
/// Wider than about one and a half and an icon starts growing while the pointer
/// is still three icons away, which reads as the row twitching at everything.
const REACH: f32 = 1.3;
const SPACING: f32 = 8.0;

pub fn run() -> cosmic::iced::Result {
    localize::localize();
    cosmic::applet::run::<HoverDock>(())
}

/// One pinned application.
struct Entry {
    id: String,
    #[allow(dead_code)]
    name: String,
    exec: String,
    terminal: bool,
    icon: fde::IconSource,
    desktop_entry: fde::DesktopEntry,
    windows: Vec<ToplevelInfo>,
}

#[derive(Debug, Clone, Copy)]
pub enum PopupKind {
    Menu,
    Windows,
}

#[derive(Debug, Clone)]
struct Popup {
    id: window::Id,
    entry_id: String,
    kind: PopupKind,
}

#[derive(Default)]
struct HoverDock {
    core: cosmic::app::Core,
    entries: Vec<Entry>,
    /// One spring per entry, in the same order.
    scales: Vec<Spring>,
    /// Where the pointer is along the row, when it is over us.
    cursor: Option<f32>,
    last_frame: Option<Instant>,
    animating: bool,
    wayland_sender: Option<calloop::channel::Sender<WaylandRequest>>,
    desktop_cache: desktop::DesktopEntryCache,
    popup: Option<Popup>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Moved(Point),
    Left,
    Pressed(usize),
    OpenPopup(usize, iced::Rectangle<f32>, PopupKind),
    ClosePopup,
    Closed(window::Id),
    Wayland(WaylandUpdate),
    Activate(ExtForeignToplevelHandleV1),
    Launch(String, bool, String),
    Move(String, isize),
    Unpin(String),
    Frame(Instant),
}

impl HoverDock {
    /// How thick the row is across the bar.
    ///
    /// Exactly one applet unit — the same as every other applet in the panel.
    /// cosmic-panel sizes its bar to the thickest applet in it and reserves
    /// that much of the screen, so a container tall enough for a fully
    /// magnified icon would push every window on the display down and leave
    /// this row sitting below the buttons beside it.
    fn thickness(&self) -> f32 {
        let (icon, padding) = (
            self.core.applet.suggested_size(false).0 as f32,
            self.core.applet.suggested_padding(false).0 as f32,
        );
        icon + padding * 2.0
    }

    fn metrics(&self) -> Metrics {
        let icon_size = self.core.applet.suggested_size(false).0 as f32;
        // The configured magnification is a wish. What an icon can actually
        // grow to is whatever fits in one applet unit, and drawing it larger
        // than that only crops it.
        let room = (self.thickness() - 2.0) / icon_size;
        Metrics {
            icon_size,
            spacing: SPACING,
            magnification: MAGNIFICATION.min(room).max(1.0),
            reach: REACH,
        }
    }

    fn is_horizontal(&self) -> bool {
        matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        )
    }

    /// The length of the row when every icon is at its largest, which is the
    /// size the container is fixed at so the surface never changes.
    fn span(&self, metrics: &Metrics) -> f32 {
        let count = self.entries.len();
        if count == 0 {
            return 1.0;
        }
        // Sampled rather than derived: the widest the row gets is a sum over a
        // falloff curve, and where the maximum falls depends on the icon count
        // and the reach. Fifty samples of the real layout cost microseconds and
        // cannot disagree with what is drawn.
        let resting = metrics.resting_width(count);
        let mut widest = resting;
        for step in 0..=50 {
            let cursor = resting * step as f32 / 50.0;
            let scales = metrics.target_scales(count, Some(cursor), resting / 2.0);
            let grown: f32 = scales.iter().map(|s| s * metrics.icon_size).sum::<f32>()
                + (count - 1) as f32 * metrics.spacing;
            widest = widest.max(grown);
        }
        widest
    }

    /// Step every spring towards where the pointer says it should be.
    fn animate(&mut self, now: Instant) {
        let metrics = self.metrics();
        let span = self.span(&metrics);
        let dt = self
            .last_frame
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0);
        self.last_frame = Some(now);

        let targets = metrics.target_scales(self.entries.len(), self.cursor, span / 2.0);
        let mut moving = false;
        for (spring, target) in self.scales.iter_mut().zip(targets.iter()) {
            moving |= spring.step(*target, dt);
        }
        self.animating = moving;
    }

    fn placed(&self) -> (Metrics, f32, Vec<Placed>) {
        let metrics = self.metrics();
        let span = self.span(&metrics);
        let scales: Vec<f32> = self.scales.iter().map(|s| s.value).collect();
        let mut placed = metrics.place(&scales, self.cursor, span / 2.0);

        // The layout anchors the row to the pointer, so at the extremes it can
        // sit far enough off centre to overhang the container. Anything outside
        // is not clipped, it simply is not drawn, and an end icon would lose a
        // slice of itself.
        if let (Some(first), Some(last)) = (placed.first(), placed.last()) {
            let (left, right) = (first.left(), last.left() + last.size);
            let shift = if left < 0.0 {
                -left
            } else if right > span {
                span - right
            } else {
                0.0
            };
            if shift != 0.0 {
                for p in &mut placed {
                    p.center += shift;
                }
            }
        }
        (metrics, span, placed)
    }

    fn read_favourites(&mut self) {
        let locales = fde::get_languages_from_env();
        let config = AppListConfig::default();
        let favorites = cosmic::cosmic_config::Config::new(
            cosmic_app_list_config::APP_ID,
            AppListConfig::VERSION,
        )
        .ok()
        .and_then(|helper| AppListConfig::get_entry(&helper).ok())
        .map(|entry| entry.favorites)
        .unwrap_or(config.favorites);

        let entries = fde::Iter::new(fde::default_paths())
            .filter_map(|path| fde::DesktopEntry::from_path(path, Some(&locales)).ok())
            .collect::<Vec<_>>();

        self.entries = favorites
            .iter()
            .filter_map(|id| {
                let entry = entries
                    .iter()
                    .find(|entry| entry.appid.as_str() == id.as_str())?;
                Some(Entry {
                    id: id.clone(),
                    name: entry
                        .name(&locales)
                        .map(|name| name.to_string())
                        .unwrap_or_else(|| id.clone()),
                    exec: entry.exec()?.to_string(),
                    terminal: entry.terminal(),
                    icon: fde::IconSource::from_unknown(entry.icon().unwrap_or_default()),
                    desktop_entry: entry.clone(),
                    windows: Vec::new(),
                })
            })
            .collect();
        self.scales = self.entries.iter().map(|_| Spring::new(1.0)).collect();
    }

    fn entry_id_for_toplevel(&mut self, info: &ToplevelInfo) -> Option<String> {
        let context = desktop::DesktopLookupContext::new(info.app_id.as_str())
            .with_identifier(info.identifier.as_str())
            .with_title(info.title.as_str());
        let resolved = desktop::resolve_desktop_entry(
            &mut self.desktop_cache,
            &context,
            &desktop::DesktopResolveOptions::default(),
        );
        self.entries
            .iter()
            .find(|entry| entry.desktop_entry.id() == resolved.id())
            .map(|entry| entry.id.clone())
    }

    fn update_toplevel(&mut self, info: ToplevelInfo) {
        for entry in &mut self.entries {
            entry
                .windows
                .retain(|window| window.foreign_toplevel != info.foreign_toplevel);
        }
        if let Some(id) = self.entry_id_for_toplevel(&info)
            && let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id)
        {
            entry.windows.push(info);
        }
    }

    fn remove_toplevel(&mut self, handle: &ExtForeignToplevelHandleV1) {
        for entry in &mut self.entries {
            entry
                .windows
                .retain(|window| &window.foreign_toplevel != handle);
        }
    }

    fn write_favourites(&self) {
        let Ok(config) = Config::new(APP_LIST_ID, AppListConfig::VERSION) else {
            tracing::error!("failed to open app-list configuration");
            return;
        };
        let mut app_list = AppListConfig::get_entry(&config).unwrap_or_default();
        app_list.update_pinned(
            self.entries.iter().map(|entry| entry.id.clone()).collect(),
            &config,
        );
    }

    fn launch(exec: String, terminal: bool, app_id: String) {
        tokio::spawn(async move {
            cosmic::desktop::spawn_desktop_exec(
                exec,
                Vec::<(&str, &str)>::new(),
                Some(app_id.as_str()),
                terminal,
            )
            .await;
        });
    }

    fn close_popup(&mut self) -> app::Task<Message> {
        self.popup
            .take()
            .map_or_else(Task::none, |popup| destroy_popup(popup.id))
    }
}

impl cosmic::Application for HoverDock {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletHoverDock";

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        let mut dock = Self {
            core,
            desktop_cache: desktop::DesktopEntryCache::new(fde::get_languages_from_env()),
            ..Default::default()
        };
        dock.desktop_cache.ensure_loaded();
        dock.read_favourites();
        (dock, app::Task::none())
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::Moved(point) => {
                self.cursor = Some(if self.is_horizontal() {
                    point.x
                } else {
                    point.y
                });
                self.animating = true;
            }
            Message::Left => {
                self.cursor = None;
                self.animating = true;
            }
            Message::Frame(now) => self.animate(now),
            Message::Pressed(index) => {
                if let Some(entry) = self.entries.get(index) {
                    match entry.windows.as_slice() {
                        [] => Self::launch(entry.exec.clone(), entry.terminal, entry.id.clone()),
                        [window] => {
                            if let Some(tx) = &self.wayland_sender {
                                let _ = tx.send(WaylandRequest::Activate(
                                    window.foreign_toplevel.clone(),
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::OpenPopup(index, rectangle, kind) => {
                if let Some(old) = self.popup.take() {
                    return destroy_popup(old.id);
                }
                let Some(entry_id) = self.entries.get(index).map(|entry| entry.id.clone()) else {
                    return Task::none();
                };
                let parent = self.core.main_window_id().expect("applet main window");
                return cosmic::surface::surface_task(cosmic::surface::action::app_popup(
                    |_| Default::default(),
                    move |app: &mut Self| {
                        let id = window::Id::unique();
                        app.popup = Some(Popup {
                            id,
                            entry_id: entry_id.clone(),
                            kind,
                        });
                        let mut settings = app
                            .core
                            .applet
                            .get_popup_settings(parent, id, None, None, None);
                        settings.positioner.anchor_rect = iced::Rectangle {
                            x: rectangle.x as i32,
                            y: rectangle.y as i32,
                            width: rectangle.width as i32,
                            height: rectangle.height as i32,
                        };
                        settings
                    },
                    None,
                ));
            }
            Message::ClosePopup => return self.close_popup(),
            Message::Closed(id) => {
                if self.popup.as_ref().is_some_and(|popup| popup.id == id) {
                    self.popup = None;
                }
            }
            Message::Wayland(update) => match update {
                WaylandUpdate::Init(tx) => self.wayland_sender = Some(tx),
                WaylandUpdate::Finished => {
                    self.wayland_sender = None;
                    tracing::error!("Wayland toplevel subscription ended");
                }
                WaylandUpdate::Toplevel(boxed) => match *boxed {
                    ToplevelUpdate::Add(info) | ToplevelUpdate::Update(info) => {
                        self.update_toplevel(info);
                    }
                    ToplevelUpdate::Remove(handle) => self.remove_toplevel(&handle),
                },
            },
            Message::Activate(handle) => {
                if let Some(tx) = &self.wayland_sender {
                    let _ = tx.send(WaylandRequest::Activate(handle));
                }
                return self.close_popup();
            }
            Message::Launch(exec, terminal, app_id) => {
                Self::launch(exec, terminal, app_id);
                return self.close_popup();
            }
            Message::Move(id, offset) => {
                if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
                    let target = index
                        .saturating_add_signed(offset)
                        .min(self.entries.len() - 1);
                    if target != index {
                        self.entries.swap(index, target);
                        self.scales.swap(index, target);
                        self.write_favourites();
                    }
                }
                return self.close_popup();
            }
            Message::Unpin(id) => {
                if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
                    self.entries.remove(index);
                    self.scales.remove(index);
                    self.write_favourites();
                }
                return self.close_popup();
            }
        }
        app::Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        // Frames only while something is moving. An idle dock that asked for
        // every frame would be a background process that spins.
        Subscription::batch([
            wayland_subscription::wayland_subscription().map(Message::Wayland),
            if self.animating {
                window::frames().map(|(_, instant)| Message::Frame(instant))
            } else {
                Subscription::none()
            },
        ])
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let (metrics, span, placed) = self.placed();
        let thickness = self.thickness();

        let icons =
            self.entries
                .iter()
                .zip(placed.iter())
                .enumerate()
                .map(|(index, (entry, placed))| {
                    let size = placed.size.round().max(1.0) as u16;
                    let icon = cosmic::widget::icon(entry.icon.as_cosmic_icon()).size(size);
                    let indicator = container(horizontal_space())
                        .width(Length::Fixed(6.0))
                        .height(Length::Fixed(6.0))
                        .class(if entry.windows.is_empty() {
                            Container::Transparent
                        } else {
                            Container::custom(|theme| container::Style {
                                background: Some(Background::Color(
                                    theme.cosmic().on_bg_color().into(),
                                )),
                                border: Border {
                                    radius: 99.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                        });
                    let indicator = container(indicator)
                        .width(Length::Fixed(placed.size))
                        .height(Length::Fixed(placed.size))
                        .align_x(match self.core.applet.anchor {
                            PanelAnchor::Left => Alignment::Start,
                            PanelAnchor::Right => Alignment::End,
                            PanelAnchor::Top | PanelAnchor::Bottom => Alignment::Center,
                        })
                        .align_y(match self.core.applet.anchor {
                            PanelAnchor::Top => Alignment::Start,
                            PanelAnchor::Bottom => Alignment::End,
                            PanelAnchor::Left | PanelAnchor::Right => Alignment::Center,
                        });
                    let icon_with_indicator: Element<'_, Message> = stack![icon, indicator].into();
                    let rectangle = if self.is_horizontal() {
                        iced::Rectangle {
                            x: placed.left(),
                            y: 0.0,
                            width: placed.size,
                            height: thickness,
                        }
                    } else {
                        iced::Rectangle {
                            x: 0.0,
                            y: placed.left(),
                            width: thickness,
                            height: placed.size,
                        }
                    };
                    let mut area = mouse_area(
                        cosmic::widget::container(icon_with_indicator)
                            .width(Length::Fixed(placed.size.max(1.0)))
                            .height(Length::Fixed(thickness))
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center),
                    );
                    area = if entry.windows.len() > 1 {
                        area.on_press(Message::OpenPopup(index, rectangle, PopupKind::Windows))
                    } else {
                        area.on_press(Message::Pressed(index))
                    };
                    area.on_right_release(Message::OpenPopup(index, rectangle, PopupKind::Menu))
                        .into()
                });

        // Fixed along the bar as well as across it: the container is as long as
        // the row can ever be, so nothing the pointer does changes the size the
        // panel is told about.
        let row: Element<'_, Message> = if self.is_horizontal() {
            Row::with_children(icons)
                .spacing(metrics.spacing)
                .align_y(Alignment::Center)
                .into()
        } else {
            Column::with_children(icons)
                .spacing(metrics.spacing)
                .align_x(Alignment::Center)
                .into()
        };

        let (width, height) = if self.is_horizontal() {
            (span, thickness)
        } else {
            (thickness, span)
        };

        let content = cosmic::widget::container(row)
            .width(Length::Fixed(width))
            .height(Length::Fixed(height));

        autosize(
            mouse_area(content)
                .on_move(Message::Moved)
                .on_exit(Message::Left),
            AUTOSIZE_MAIN_ID.clone(),
        )
        .into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Self::Message> {
        let Some(popup) = self.popup.as_ref().filter(|popup| popup.id == id) else {
            return self.core.applet.popup_container(text::body("")).into();
        };
        let Some(entry) = self.entries.iter().find(|entry| entry.id == popup.entry_id) else {
            return self.core.applet.popup_container(text::body("")).into();
        };

        fn menu_button<'a>(
            label: impl Into<Cow<'a, str>> + 'a,
        ) -> cosmic::widget::Button<'a, Message> {
            button::custom(text::body(label))
                .height(20 + 2 * theme::spacing().space_xxs)
                .class(Button::MenuItem)
                .padding([theme::spacing().space_xxs, theme::spacing().space_s])
                .width(Length::Fill)
        }

        let mut content = Column::new().padding(8).spacing(2).width(Length::Fill);
        match popup.kind {
            PopupKind::Windows => {
                for window in &entry.windows {
                    content = content.push(
                        menu_button(window.title.clone())
                            .on_press(Message::Activate(window.foreign_toplevel.clone())),
                    );
                }
            }
            PopupKind::Menu => {
                for window in &entry.windows {
                    content = content.push(
                        menu_button(window.title.clone())
                            .on_press(Message::Activate(window.foreign_toplevel.clone())),
                    );
                }
                if !entry.windows.is_empty() {
                    content = content.push(divider::horizontal::light());
                }

                content = content.push(
                    menu_button(if entry.windows.is_empty() {
                        fl!("open", app = entry.name.as_str())
                    } else {
                        fl!("new-window", app = entry.name.as_str())
                    })
                    .on_press(Message::Launch(
                        entry.exec.clone(),
                        entry.terminal,
                        entry.id.clone(),
                    )),
                );
                for action in entry.desktop_entry.actions().into_iter().flatten() {
                    if action == "new-window" {
                        continue;
                    }
                    let Some(exec) = entry.desktop_entry.action_entry(action, "Exec") else {
                        continue;
                    };
                    let Some(name) = entry.desktop_entry.action_entry_localized(
                        action,
                        "Name",
                        self.desktop_cache.locales(),
                    ) else {
                        continue;
                    };
                    content = content.push(menu_button(name).on_press(Message::Launch(
                        exec.into(),
                        entry.terminal,
                        entry.id.clone(),
                    )));
                }

                content = content.push(divider::horizontal::light());
                let index = self
                    .entries
                    .iter()
                    .position(|item| item.id == entry.id)
                    .unwrap_or(0);
                if index > 0 {
                    content = content.push(
                        menu_button(fl!("move-backward"))
                            .on_press(Message::Move(entry.id.clone(), -1)),
                    );
                }
                if index + 1 < self.entries.len() {
                    content = content.push(
                        menu_button(fl!("move-forward"))
                            .on_press(Message::Move(entry.id.clone(), 1)),
                    );
                }
                content = content
                    .push(menu_button(fl!("unpin")).on_press(Message::Unpin(entry.id.clone())));
            }
        }

        self.core
            .applet
            .popup_container(container(content).width(Length::Fixed(300.0)))
            .limits(
                Limits::NONE
                    .min_width(180.0)
                    .min_height(1.0)
                    .max_width(360.0)
                    .max_height(1000.0),
            )
            .into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Self::Message> {
        Some(Message::Closed(id))
    }
}
