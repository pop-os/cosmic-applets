// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::LazyLock;

use cosmic::{
    Element, Renderer, Theme,
    cosmic_theme::Spacing,
    iced::{Alignment, Color, Length, Point, Rectangle, Size, mouse},
    theme,
    widget::{button, canvas, column, container, indeterminate_circular, row, space, text},
};

use crate::{
    app::Message,
    fl,
    stats::{self, InternetPing, LiveStats, RATE_HISTORY},
};

const GRAPH_HEIGHT: f32 = 80.0;

struct Labels {
    receiving: String,
    sending: String,
    ping: String,
    packet_loss: String,
    dns: String,
    ip_address: String,
    downloaded: String,
    uploaded: String,
    gateway: String,
    no_value: String,
    ping_internet: String,
    pinging: String,
    ping_failed: String,
}

static LABELS: LazyLock<Labels> = LazyLock::new(|| Labels {
    receiving: fl!("receiving"),
    sending: fl!("sending"),
    ping: fl!("ping"),
    packet_loss: fl!("packet-loss"),
    dns: fl!("dns"),
    ip_address: fl!("ip-address"),
    downloaded: fl!("downloaded"),
    uploaded: fl!("uploaded"),
    gateway: fl!("gateway"),
    no_value: fl!("no-value"),
    ping_internet: fl!("ping-internet"),
    pinging: fl!("pinging"),
    ping_failed: fl!("ping-failed"),
});

#[derive(Default)]
pub struct GraphCaches {
    pub rx: canvas::Cache,
    pub tx: canvas::Cache,
}

impl GraphCaches {
    pub fn clear(&self) {
        self.rx.clear();
        self.tx.clear();
    }
}

pub fn details_panel<'a>(
    stats: &'a LiveStats,
    spacing: Spacing,
    caches: &'a GraphCaches,
) -> Element<'a, Message> {
    let labels = &*LABELS;
    let missing = labels.no_value.as_str();

    let rx_rate = if stats.has_rates {
        stats::format_rate(stats.rx_rate)
    } else {
        missing.to_string()
    };
    let tx_rate = if stats.has_rates {
        stats::format_rate(stats.tx_rate)
    } else {
        missing.to_string()
    };
    let downloaded = if stats.interface.is_some() {
        stats::format_bytes(stats.rx_bytes)
    } else {
        missing.to_string()
    };
    let uploaded = if stats.interface.is_some() {
        stats::format_bytes(stats.tx_bytes)
    } else {
        missing.to_string()
    };

    let ping = stats::format_ping_ms(stats.router_ping_ms);
    let loss = stats::format_percent(stats.packet_loss_percent());
    let dns = stats::format_dns(stats);
    let gateway = stats.gateway.as_deref().unwrap_or(missing).to_string();
    let ip = stats.ip.as_deref().unwrap_or(missing).to_string();

    container(
        column::with_children([
            rate_chart(
                labels.receiving.clone(),
                rx_rate,
                &stats.rx_history,
                &caches.rx,
                spacing.space_xxs,
            ),
            rate_chart(
                labels.sending.clone(),
                tx_rate,
                &stats.tx_history,
                &caches.tx,
                spacing.space_xxs,
            ),
            row::with_children([
                stat_cell(labels.ping.clone(), ping),
                internet_ping_row(&stats.internet_ping, spacing),
            ])
            .spacing(spacing.space_s)
            .align_y(Alignment::Start)
            .into(),
            stats_grid(
                [
                    (labels.packet_loss.clone(), loss),
                    (labels.dns.clone(), dns),
                    (labels.ip_address.clone(), ip),
                    (labels.gateway.clone(), gateway),
                    (labels.downloaded.clone(), downloaded),
                    (labels.uploaded.clone(), uploaded),
                ],
                spacing,
            ),
        ])
        .spacing(spacing.space_s),
    )
    .class(theme::Container::Card)
    .padding(spacing.space_s)
    .width(Length::Fill)
    .into()
}

fn rate_chart<'a>(
    label: String,
    rate: String,
    history: &'a std::collections::VecDeque<f64>,
    cache: &'a canvas::Cache,
    space_xxs: u16,
) -> Element<'a, Message> {
    let header = row::with_children([
        text::body(label).into(),
        space::horizontal().into(),
        text::heading(rate).into(),
    ])
    .align_y(Alignment::Center);

    column::with_children([
        header.into(),
        canvas(RateGraph::from_history(history, cache))
            .height(Length::Fixed(GRAPH_HEIGHT))
            .width(Length::Fill)
            .into(),
    ])
    .spacing(space_xxs)
    .into()
}

struct RateGraph<'a> {
    values: Vec<f32>,
    cache: &'a canvas::Cache,
}

impl<'a> RateGraph<'a> {
    fn from_history(history: &std::collections::VecDeque<f64>, cache: &'a canvas::Cache) -> Self {
        let mut values = vec![0.0; RATE_HISTORY.saturating_sub(history.len())];
        values.extend(history.iter().map(|value| *value as f32));
        Self { values, cache }
    }
}

impl canvas::Program<Message, Theme, Renderer> for RateGraph<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let cosmic = theme.cosmic();
        let accent = Color::from(cosmic.accent_color());
        let mut accent_fill = accent;
        accent_fill.a *= 0.5;
        let bg = Color::from(cosmic.bg_component_color());
        let border = Color::from(cosmic.bg_component_divider());
        let radius = cosmic.radius_0();

        vec![self.cache.draw(renderer, bounds.size(), |frame| {
            let mut max = 0.0f32;
            for value in &self.values {
                max = max.max(*value);
            }
            let scale_y = 10.0f32.powf(max.max(1.0).log10().ceil().max(2.0));

            let invalid_is_zero = |value: f32| if value.is_finite() { value } else { 0.0 };
            let n = self.values.len().max(1);
            let calc_x = |i: usize| -> f32 {
                if n == 1 {
                    bounds.width
                } else {
                    i as f32 / (n - 1) as f32 * bounds.width
                }
            };
            let calc_y =
                |value: f32| -> f32 { (1.0 - invalid_is_zero(value / scale_y)) * bounds.height };

            let background = canvas::Path::rounded_rectangle(
                Point::ORIGIN,
                Size::new(bounds.width, bounds.height),
                radius.into(),
            );
            frame.fill(&background, bg);
            frame.stroke(&background, canvas::Stroke::default().with_color(border));

            for fraction in [0.25, 0.5, 0.75] {
                let y = calc_y(scale_y * fraction);
                let grid = canvas::Path::line(Point::new(0.0, y), Point::new(bounds.width, y));
                frame.stroke(&grid, canvas::Stroke::default().with_color(accent_fill));
            }

            if !self.values.is_empty() {
                let mut area = canvas::path::Builder::new();
                let mut line = canvas::path::Builder::new();
                area.move_to(Point::new(0.0, bounds.height));
                for (i, value) in self.values.iter().enumerate() {
                    let point = Point::new(calc_x(i), calc_y(*value));
                    area.line_to(point);
                    if i == 0 {
                        line.move_to(point);
                    } else {
                        line.line_to(point);
                    }
                }
                area.line_to(Point::new(bounds.width, bounds.height));
                area.close();
                frame.fill(&area.build(), accent_fill);
                frame.stroke(&line.build(), canvas::Stroke::default().with_color(accent));
            }
        })]
    }
}

fn internet_ping_row<'a>(state: &InternetPing, spacing: Spacing) -> Element<'a, Message> {
    let labels = &*LABELS;
    let mut children: Vec<Element<'a, Message>> = Vec::with_capacity(2);
    match state {
        InternetPing::Running => {
            children.push(
                row::with_children([
                    indeterminate_circular().size(16.0).into(),
                    text::body(labels.pinging.clone()).into(),
                ])
                .spacing(spacing.space_xxs)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        InternetPing::Idle | InternetPing::Done { .. } => {
            children.push(
                button::standard(labels.ping_internet.clone())
                    .on_press(Message::PingInternet)
                    .width(Length::Fill)
                    .into(),
            );
        }
    }
    if let InternetPing::Done { avg_ms, loss } = state {
        let result = match (avg_ms, loss) {
            (Some(avg), Some(loss)) => {
                fl!(
                    "ping-result",
                    avg = stats::format_ping_ms(Some(*avg)),
                    loss = loss
                )
            }
            _ => labels.ping_failed.clone(),
        };
        children.push(text::heading(result).into());
    }
    column::with_children(children)
        .spacing(spacing.space_xxs)
        .width(Length::Fill)
        .into()
}

fn stats_grid<'a, const N: usize>(
    cells: [(String, String); N],
    spacing: Spacing,
) -> Element<'a, Message> {
    let mut rows = Vec::new();
    let mut iter = cells.into_iter();
    while let Some((l1, v1)) = iter.next() {
        let left = stat_cell(l1, v1);
        let right = if let Some((l2, v2)) = iter.next() {
            stat_cell(l2, v2)
        } else {
            space::horizontal().into()
        };
        rows.push(
            row::with_children([left, right])
                .spacing(spacing.space_s)
                .into(),
        );
    }
    column::with_children(rows).spacing(spacing.space_xs).into()
}

fn stat_cell<'a>(label: String, value: String) -> Element<'a, Message> {
    column::with_children([text::body(label).into(), text::heading(value).into()])
        .width(Length::Fill)
        .into()
}
