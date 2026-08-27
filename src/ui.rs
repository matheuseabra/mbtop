//! Dashboard rendering: turns a [`Stats`] snapshot into Ratatui widgets.
//!
//! Rendering is a pure function of the snapshot, so tests can draw into a
//! [`TestBackend`] buffer and assert on the cells.

use std::collections::VecDeque;

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, LineGauge, Paragraph, Sparkline, SparklineBar},
    Frame,
};

use crate::app::Stats;
use crate::metrics;

/// Narrowest pane width that fits the dashboard.
pub const MIN_WIDTH: u16 = 36;
/// Shortest pane height that fits the dashboard.
pub const MIN_HEIGHT: u16 = 8;

#[derive(Debug, Clone, Copy)]
enum Metric {
    Cpu,
    Memory,
    Disk,
    Load,
    Network,
}

/// Renders the dashboard, or a centered resize hint below the minimum size.
#[allow(clippy::too_many_lines)]
pub fn render(frame: &mut Frame, stats: &Stats) {
    let screen = frame.area();
    if screen.width < MIN_WIDTH || screen.height < MIN_HEIGHT {
        render_too_small(frame, screen);
        return;
    }
    let [area, _] =
        Layout::vertical([Constraint::Length(MIN_HEIGHT), Constraint::Fill(1)]).areas(screen);

    let title = Line::from(vec![
        Span::styled(" mbtop ", title_style()),
        Span::styled(format!("· {} ", stats.host), label_style()),
    ]);
    let block = if stats.borderless {
        Block::new().title(title)
    } else {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(default_style())
            .title(title)
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [cpu_area, memory_area, disk_area, details_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);

    render_cpu(frame, cpu_area, stats.cpu, stats.cpu_history, stats.icons);

    let memory_percent = metrics::percentage(stats.memory_used, stats.memory_total);
    render_gauge(
        frame,
        memory_area,
        Metric::Memory,
        format!(
            "{} / {}",
            metrics::bytes(stats.memory_used),
            metrics::bytes(stats.memory_total)
        ),
        memory_percent,
        stats.icons,
    );

    if let Some(disk) = stats.disk {
        render_gauge(
            frame,
            disk_area,
            Metric::Disk,
            format!(
                "{} / {}",
                metrics::bytes(disk.used),
                metrics::bytes(disk.total)
            ),
            disk.percent,
            stats.icons,
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{} ", metric_name(Metric::Disk, stats.icons)),
                    metric_style(stats.icons),
                ),
                Span::styled("unavailable", muted_style()),
            ])),
            disk_area,
        );
    }

    let [load_area, network_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(details_area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", metric_name(Metric::Load, stats.icons)),
                metric_style(stats.icons),
            ),
            Span::raw(format!(
                "{:.2} {:.2} {:.2}",
                stats.load.0, stats.load.1, stats.load.2
            )),
        ])),
        load_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", metric_name(Metric::Network, stats.icons)),
                metric_style(stats.icons),
            ),
            Span::raw(format!(
                "↓{} ↑{}",
                metrics::bytes(stats.network.received),
                metrics::bytes(stats.network.transmitted)
            )),
        ])),
        network_area,
    );

    let [_, footer] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(footer_area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("up ", label_style()),
            Span::styled(
                metrics::format_uptime(stats.uptime_seconds),
                default_style(),
            ),
            Span::styled(" · ", label_style()),
            Span::styled(format!("{} cores", stats.cores), default_style()),
            Span::styled(" · ", label_style()),
            Span::styled("q quit", label_style()),
        ])),
        footer,
    );
}

fn render_cpu(frame: &mut Frame, area: Rect, cpu: f32, history: &VecDeque<f32>, icons: bool) {
    let [value_area, chart_area] = Layout::horizontal([
        Constraint::Length(if icons { 9 } else { 15 }),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", metric_name(Metric::Cpu, icons)),
                metric_style(icons),
            ),
            Span::styled(format!("{cpu:>5.1}%"), value_style(cpu)),
        ])),
        value_area,
    );

    let data: Vec<SparklineBar> = history
        .iter()
        .map(|value| {
            SparklineBar::from((*value).clamp(0.0, 100.0) as u64)
                .style(Some(Style::default().fg(gradient_color(*value))))
        })
        .collect();
    frame.render_widget(
        Sparkline::default()
            .data(data)
            .max(100)
            .bar_set(symbols::bar::NINE_LEVELS)
            .style(default_style()),
        chart_area,
    );
}

fn render_gauge(
    frame: &mut Frame,
    area: Rect,
    metric: Metric,
    value: String,
    percent: f32,
    icons: bool,
) {
    let gauge = LineGauge::default()
        .label(Line::from(vec![
            Span::styled(
                format!("{} ", metric_name(metric, icons)),
                metric_style(icons),
            ),
            Span::styled(value, value_style(percent)),
        ]))
        .ratio(f64::from((percent / 100.0).clamp(0.0, 1.0)))
        .filled_symbol("━")
        .unfilled_symbol("─")
        .filled_style(Style::default().fg(gradient_color(percent)))
        .unfilled_style(muted_style());
    frame.render_widget(gauge, area);
}

fn render_too_small(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);
    let title = Line::from(Span::styled("mbtop needs more room", title_style()));

    if area.height >= 3 {
        let message = vec![
            title,
            Line::from(Span::styled(
                format!(
                    "minimum {}×{}, now {}×{}",
                    MIN_WIDTH, MIN_HEIGHT, area.width, area.height
                ),
                muted_style(),
            )),
        ];
        let [_, centered, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(message).alignment(Alignment::Center),
            centered,
        );
    } else {
        frame.render_widget(Paragraph::new(title).alignment(Alignment::Center), area);
    }
}

fn default_style() -> Style {
    Style::default().fg(Color::Reset)
}

fn title_style() -> Style {
    default_style().add_modifier(Modifier::BOLD)
}

fn muted_style() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM)
}

fn label_style() -> Style {
    Style::default().fg(Color::Gray)
}

fn metric_style(icons: bool) -> Style {
    let style = label_style();
    if icons {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn value_style(percent: f32) -> Style {
    Style::default().fg(gradient_color(percent))
}

fn gradient_color(percent: f32) -> Color {
    match percent {
        p if p >= 90.0 => Color::Red,
        p if p >= 65.0 => Color::Yellow,
        p if p >= 35.0 => Color::Green,
        _ => Color::Cyan,
    }
}

fn metric_name(metric: Metric, icons: bool) -> &'static str {
    if icons {
        match metric {
            Metric::Cpu => "▣",
            Metric::Memory => "▥",
            Metric::Disk => "▭",
            Metric::Load => "≋",
            Metric::Network => "↕",
        }
    } else {
        match metric {
            Metric::Cpu => "cpu",
            Metric::Memory => "mem",
            Metric::Disk => "disk",
            Metric::Load => "load",
            Metric::Network => "net",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use ratatui::{backend::TestBackend, Terminal};

    use super::*;
    use crate::app::Stats;
    use crate::metrics::{DiskUsage, NetworkUsage};

    static EMPTY_HISTORY: VecDeque<f32> = VecDeque::new();

    fn sample_stats() -> Stats<'static> {
        Stats {
            host: "test-host",
            cpu: 42.0,
            cpu_history: &EMPTY_HISTORY,
            cores: 8,
            memory_used: 12 * 1024 * 1024 * 1024,
            memory_total: 16 * 1024 * 1024 * 1024,
            disk: Some(DiskUsage {
                total: 460 * 1024 * 1024 * 1024,
                used: 45 * 1024 * 1024 * 1024,
                percent: 9.8,
            }),
            load: (4.84, 4.27, 4.44),
            network: NetworkUsage {
                received: 0,
                transmitted: 0,
            },
            uptime_seconds: 4 * 86_400 + 5 * 60,
            icons: false,
            borderless: false,
        }
    }

    fn draw(width: u16, height: u16, stats: &Stats) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, stats)).unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn dashboard_renders_every_metric_row() {
        let screen = draw(80, 12, &sample_stats());
        for label in ["cpu", "mem", "disk", "load", "net", "q quit", "test-host"] {
            assert!(screen.contains(label), "missing {label} in:\n{screen}");
        }
    }

    #[test]
    fn borderless_mode_omits_outer_border() {
        let stats = Stats {
            borderless: true,
            ..sample_stats()
        };
        let screen = draw(80, 12, &stats);
        assert!(screen.contains("mbtop"), "title missing in:\n{screen}");
        assert!(!screen.contains("╭"), "border present in:\n{screen}");
    }

    #[test]
    fn dashboard_formats_values_through_metrics() {
        let screen = draw(80, 12, &sample_stats());
        assert!(screen.contains("42.0%"), "cpu value missing in:\n{screen}");
        assert!(
            screen.contains("12.0G / 16.0G"),
            "memory values missing in:\n{screen}"
        );
        assert!(
            screen.contains("4d 00h 05m"),
            "uptime missing in:\n{screen}"
        );
    }

    #[test]
    fn icon_mode_uses_glyph_labels() {
        let stats = Stats {
            icons: true,
            ..sample_stats()
        };
        let screen = draw(80, 12, &stats);
        assert!(screen.contains("▣"), "glyphs missing in:\n{screen}");
    }

    #[test]
    fn missing_disk_renders_an_unavailable_placeholder() {
        let stats = Stats {
            disk: None,
            ..sample_stats()
        };
        let screen = draw(80, 12, &stats);
        assert!(
            screen.contains("unavailable"),
            "placeholder missing in:\n{screen}"
        );
    }

    #[test]
    fn out_of_range_values_do_not_panic_the_gauge() {
        let stats = Stats {
            memory_used: 20 * 1024 * 1024 * 1024,
            memory_total: 16 * 1024 * 1024 * 1024,
            ..sample_stats()
        };
        let screen = draw(80, 12, &stats);
        assert!(
            screen.contains("20.0G / 16.0G"),
            "values missing in:\n{screen}"
        );
    }

    #[test]
    fn panes_below_the_minimum_show_the_resize_hint() {
        let screen = draw(MIN_WIDTH - 1, MIN_HEIGHT - 1, &sample_stats());
        assert!(
            screen.contains("mbtop needs more room"),
            "hint missing in:\n{screen}"
        );
        assert!(
            screen.contains("minimum 36×8, now 35×7"),
            "dimensions missing in:\n{screen}"
        );
    }

    #[test]
    fn very_short_panes_show_only_the_title() {
        let screen = draw(MIN_WIDTH - 1, 2, &sample_stats());
        assert!(screen.contains("mbtop needs more room"));
        assert!(!screen.contains("minimum 36×8"));
    }

    #[test]
    fn gradient_uses_terminal_palette_entries() {
        assert_eq!(
            [
                gradient_color(10.0),
                gradient_color(34.9),
                gradient_color(35.0),
                gradient_color(64.9),
                gradient_color(65.0),
                gradient_color(89.9),
                gradient_color(90.0),
            ],
            [
                Color::Cyan,
                Color::Cyan,
                Color::Green,
                Color::Green,
                Color::Yellow,
                Color::Yellow,
                Color::Red,
            ]
        );
    }

    #[test]
    fn text_metric_labels_are_compact() {
        assert_eq!(
            [
                metric_name(Metric::Cpu, false),
                metric_name(Metric::Memory, false),
                metric_name(Metric::Disk, false),
                metric_name(Metric::Load, false),
                metric_name(Metric::Network, false),
            ],
            ["cpu", "mem", "disk", "load", "net"]
        );
    }

    #[test]
    fn icon_metric_labels_are_monochrome_symbols() {
        assert_eq!(
            [
                metric_name(Metric::Cpu, true),
                metric_name(Metric::Memory, true),
                metric_name(Metric::Disk, true),
                metric_name(Metric::Load, true),
                metric_name(Metric::Network, true),
            ],
            ["▣", "▥", "▭", "≋", "↕"]
        );
    }
}
