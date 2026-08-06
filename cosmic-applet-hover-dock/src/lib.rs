// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

//! An applet that shows the pinned applications with hover magnification.
//!
//! It reads the same favourites `cosmic-app-list` does, so the two show the
//! same applications in the same order and either can be put in the dock.
//!
//! # Why the surface is a fixed size
//!
//! cosmic-panel sizes its bar to the thickest applet in it. If the surface grew
//! as an icon magnified, the panel would re-lay out the whole bar on every
//! frame of the animation. So the container is fixed at the largest an icon can
//! get and the icons are sized inside it, which leaves `autosize` reporting the
//! same size throughout.
//!
//! The row is laid out along one axis and only becomes horizontal or vertical
//! where it is turned into widgets, so a panel on any of the four edges shares
//! one implementation.

mod layout;
mod localize;

use std::time::Instant;

use cosmic::{
    Element, app,
    applet::cosmic_panel_config::PanelAnchor,
    desktop::{IconSourceExt, fde},
    iced::{
        self, Alignment, Length, Point, Subscription,
        id::Id as WidgetId,
        widget::{Column, Row},
        window,
    },
    widget::{autosize::autosize, mouse_area},
};
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic_app_list_config::AppListConfig;
use std::sync::LazyLock;

use layout::{Metrics, Placed, Spring};

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
}

#[derive(Debug, Clone)]
pub enum Message {
    Moved(Point),
    Left,
    Pressed(usize),
    Frame(Instant),
}

impl HoverDock {
    fn metrics(&self) -> Metrics {
        let icon_size = self.core.applet.suggested_size(false).0 as f32;
        Metrics {
            icon_size,
            spacing: SPACING,
            magnification: MAGNIFICATION,
            reach: REACH,
            padding: self.core.applet.suggested_padding(false).0 as f32,
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
                })
            })
            .collect();
        self.scales = self.entries.iter().map(|_| Spring::new(1.0)).collect();
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
            ..Default::default()
        };
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
                self.cursor = Some(if self.is_horizontal() { point.x } else { point.y });
                self.animating = true;
            }
            Message::Left => {
                self.cursor = None;
                self.animating = true;
            }
            Message::Frame(now) => self.animate(now),
            Message::Pressed(index) => {
                if let Some(entry) = self.entries.get(index) {
                    let (exec, app_id, terminal) =
                        (entry.exec.clone(), entry.id.clone(), entry.terminal);
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
            }
        }
        app::Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        // Frames only while something is moving. An idle dock that asked for
        // every frame would be a background process that spins.
        if self.animating {
            window::frames().map(|(_, instant)| Message::Frame(instant))
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let (metrics, span, placed) = self.placed();
        let thickness = metrics.max_icon_size() + metrics.padding * 2.0;

        let icons = self.entries.iter().zip(placed.iter()).enumerate().map(
            |(index, (entry, placed))| {
                let size = placed.size.round().max(1.0) as u16;
                let icon = cosmic::widget::icon(entry.icon.as_cosmic_icon()).size(size);
                mouse_area(
                    cosmic::widget::container(icon)
                        .width(Length::Fixed(placed.size.max(1.0)))
                        .height(Length::Fixed(thickness))
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center),
                )
                .on_press(Message::Pressed(index))
                .into()
            },
        );

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
}
