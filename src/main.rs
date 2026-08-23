use std::{
    collections::VecDeque,
    env, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, LineGauge, Paragraph, Sparkline, SparklineBar},
    Frame, Terminal,
};
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind, System};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HISTORY_LEN: usize = 28;
const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);
const MIN_WIDTH: u16 = 36;
const MIN_HEIGHT: u16 = 8;

#[derive(Debug, Clone, Copy)]
enum Metric {
    Cpu,
    Memory,
    Disk,
    Load,
    Network,
}

#[derive(Debug, Clone)]
struct Config {
    disk_path: Option<PathBuf>,
    interval: Duration,
    icons: bool,
}

#[derive(Debug, Default, Clone, Copy)]
struct NetworkUsage {
    received: u64,
    transmitted: u64,
}

struct App {
    config: Config,
    host: String,
    system: System,
    disks: Disks,
    networks: Networks,
    cpu_history: VecDeque<f32>,
    last_network: NetworkUsage,
    network_rate: NetworkUsage,
    last_sample: Option<Instant>,
}

impl App {
    fn new(config: Config) -> Self {
        let refresh = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything());

        Self {
            config,
            host: System::host_name().unwrap_or_else(|| "local".to_string()),
            system: System::new_with_specifics(refresh),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            cpu_history: VecDeque::with_capacity(HISTORY_LEN),
            last_network: NetworkUsage::default(),
            network_rate: NetworkUsage::default(),
            last_sample: None,
        }
    }

    fn sample(&mut self) {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.disks.refresh(true);
        self.networks.refresh(true);

        let cpu = self.system.global_cpu_usage();
        if self.cpu_history.len() == HISTORY_LEN {
            self.cpu_history.pop_front();
        }
        self.cpu_history.push_back(cpu);

        let network = network_totals(&self.networks);
        if let Some(previous) = self.last_sample {
            let elapsed = previous.elapsed().as_secs_f64().max(0.001);
            self.network_rate = NetworkUsage {
                received: rate(
                    network.received.saturating_sub(self.last_network.received),
                    elapsed,
                ),
                transmitted: rate(
                    network
                        .transmitted
                        .saturating_sub(self.last_network.transmitted),
                    elapsed,
                ),
            };
        }
        self.last_network = network;
        self.last_sample = Some(Instant::now());
    }

    fn render(&self, frame: &mut Frame) {
        let screen = frame.area();
        if screen.width < MIN_WIDTH || screen.height < MIN_HEIGHT {
            render_too_small(frame, screen);
            return;
        }
        let [area, _] =
            Layout::vertical([Constraint::Length(MIN_HEIGHT), Constraint::Fill(1)]).areas(screen);

        let title = Line::from(vec![
            Span::styled(" mbtop ", title_style()),
            Span::styled(format!("· {} ", self.host), label_style()),
        ]);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(default_style())
            .title(title);
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

        render_cpu(
            frame,
            cpu_area,
            self.system.global_cpu_usage(),
            &self.cpu_history,
            self.config.icons,
        );

        let memory_percent = percentage(self.system.used_memory(), self.system.total_memory());
        render_gauge(
            frame,
            memory_area,
            Metric::Memory,
            format!(
                "{} / {}",
                bytes(self.system.used_memory()),
                bytes(self.system.total_memory())
            ),
            memory_percent,
            self.config.icons,
        );

        if let Some(disk) = disk_usage(&self.disks, self.config.disk_path.as_deref()) {
            render_gauge(
                frame,
                disk_area,
                Metric::Disk,
                format!("{} / {}", bytes(disk.used), bytes(disk.total)),
                disk.percent,
                self.config.icons,
            );
        } else {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!("{} ", metric_name(Metric::Disk, self.config.icons)),
                        metric_style(self.config.icons),
                    ),
                    Span::styled("unavailable", muted_style()),
                ])),
                disk_area,
            );
        }

        let [load_area, network_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(details_area);
        let load = System::load_average();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{} ", metric_name(Metric::Load, self.config.icons)),
                    metric_style(self.config.icons),
                ),
                Span::raw(format!(
                    "{:.2} {:.2} {:.2}",
                    load.one, load.five, load.fifteen
                )),
            ])),
            load_area,
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{} ", metric_name(Metric::Network, self.config.icons)),
                    metric_style(self.config.icons),
                ),
                Span::raw(format!(
                    "↓{} ↑{}",
                    bytes(self.network_rate.received),
                    bytes(self.network_rate.transmitted)
                )),
            ])),
            network_area,
        );

        let [_, footer] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(footer_area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("up ", label_style()),
                Span::styled(uptime(), default_style()),
                Span::styled(" · ", label_style()),
                Span::styled(
                    format!("{} cores", self.system.cpus().len()),
                    default_style(),
                ),
                Span::styled(" · ", label_style()),
                Span::styled("q quit", label_style()),
            ])),
            footer,
        );
    }
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
            .data(data.iter().cloned())
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
        .ratio((percent / 100.0).clamp(0.0, 1.0) as f64)
        .filled_symbol("━")
        .unfilled_symbol("─")
        .filled_style(Style::default().fg(gradient_color(percent)))
        .unfilled_style(muted_style());
    frame.render_widget(gauge, area);
}

fn render_too_small(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);
    let message = vec![
        Line::from(Span::styled("mbtop needs more room", title_style())),
        Line::from(Span::styled(
            format!(
                "minimum {}×{}, now {}×{}",
                MIN_WIDTH, MIN_HEIGHT, area.width, area.height
            ),
            muted_style(),
        )),
    ];

    if area.height >= 3 {
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
        frame.render_widget(
            Paragraph::new(message[0].clone()).alignment(Alignment::Center),
            area,
        );
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

fn disk_usage(disks: &Disks, requested: Option<&Path>) -> Option<DiskUsage> {
    let disk = match requested {
        Some(path) => disks
            .iter()
            .find(|disk| disk.mount_point() == path)
            .or_else(|| {
                disks
                    .iter()
                    .find(|disk| path.starts_with(disk.mount_point()))
            }),
        None => disks
            .iter()
            .find(|disk| disk.mount_point() == Path::new("/"))
            .or_else(|| disks.first()),
    }?;

    let total = disk.total_space();
    let available = disk.available_space();
    Some(DiskUsage {
        total,
        used: total.saturating_sub(available),
        percent: percentage(total.saturating_sub(available), total),
    })
}

#[derive(Debug, Clone, Copy)]
struct DiskUsage {
    total: u64,
    used: u64,
    percent: f32,
}

fn network_totals(networks: &Networks) -> NetworkUsage {
    networks
        .iter()
        .filter(|(name, _)| !name.starts_with("lo"))
        .fold(NetworkUsage::default(), |total, (_, data)| NetworkUsage {
            received: total.received.saturating_add(data.received()),
            transmitted: total.transmitted.saturating_add(data.transmitted()),
        })
}

fn rate(bytes: u64, seconds: f64) -> u64 {
    (bytes as f64 / seconds).round() as u64
}

fn percentage(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0) as f32
    }
}

fn bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = value as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0}{unit_name}", unit_name = UNITS[unit])
    } else {
        format!("{value:.1}{unit_name}", unit_name = UNITS[unit])
    }
}

fn uptime() -> String {
    let mut seconds = System::uptime();
    let days = seconds / 86_400;
    seconds %= 86_400;
    let hours = seconds / 3_600;
    seconds %= 3_600;
    let minutes = seconds / 60;

    match (days, hours, minutes) {
        (d, _, _) if d > 0 => format!("{d}d {hours:02}h {minutes:02}m"),
        (_, h, _) if h > 0 => format!("{h}h {minutes:02}m"),
        _ => format!("{minutes}m"),
    }
}

fn parse_args() -> Result<Option<Config>, String> {
    let mut disk_path = None;
    let mut interval = DEFAULT_INTERVAL;
    let mut icons = false;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("mbtop {VERSION}");
                return Ok(None);
            }
            "-d" | "--disk" => {
                disk_path = Some(PathBuf::from(args.next().ok_or("--disk needs a path")?));
            }
            "-i" | "--interval" => {
                let milliseconds: u64 = args
                    .next()
                    .ok_or("--interval needs milliseconds")?
                    .parse()
                    .map_err(|_| "--interval must be a positive number of milliseconds")?;
                if milliseconds == 0 {
                    return Err("--interval must be greater than zero".to_string());
                }
                interval = Duration::from_millis(milliseconds);
            }
            "--icon" => icons = true,
            unknown => return Err(format!("unknown argument: {unknown}\nTry 'mbtop --help'")),
        }
    }

    Ok(Some(Config {
        disk_path,
        interval,
        icons,
    }))
}

fn print_help() {
    println!(
        "mbtop {VERSION} — tiny system monitor for terminal dashboard panes\n\n\
Usage: mbtop [OPTIONS]\n\n\
Options:\n\
  -d, --disk <PATH>       Disk mount or path to report (default: /)\n\
  -i, --interval <MS>     Refresh interval in milliseconds (default: 1000)\n\
      --icon              Use compact Unicode glyphs instead of text labels\n\
  -h, --help              Show this help\n\
  -V, --version           Show version\n\n\
Keys: q / Esc / Ctrl-C to quit"
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(config) = parse_args()? else {
        return Ok(());
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    let _terminal = TerminalGuard;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(config);
    app.sample();
    terminal.draw(|frame| app.render(frame))?;
    let mut next_sample = Instant::now() + app.config.interval;

    loop {
        let until_sample = next_sample.saturating_duration_since(Instant::now());
        if event::poll(until_sample.min(Duration::from_millis(100)))? {
            match event::read()? {
                Event::Key(KeyEvent {
                    code, modifiers, ..
                }) if matches!(code, KeyCode::Char('q') | KeyCode::Esc)
                    || (code == KeyCode::Char('c')
                        && modifiers.contains(KeyModifiers::CONTROL)) =>
                {
                    break;
                }
                Event::Resize(_, _) => {
                    terminal.draw(|frame| app.render(frame))?;
                }
                _ => {}
            }
        }
        if Instant::now() >= next_sample {
            app.sample();
            terminal.draw(|frame| app.render(frame))?;
            next_sample = Instant::now() + app.config.interval;
        }
    }

    Ok(())
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_compactly() {
        assert_eq!(bytes(0), "0B");
        assert_eq!(bytes(1024), "1.0K");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0M");
    }

    #[test]
    fn percentage_handles_empty_metrics() {
        assert_eq!(percentage(1, 0), 0.0);
        assert_eq!(percentage(25, 100), 25.0);
    }

    #[test]
    fn chart_gradient_uses_terminal_palette_entries() {
        assert_eq!(gradient_color(10.0), Color::Cyan);
        assert_eq!(gradient_color(70.0), Color::Yellow);
        assert_eq!(gradient_color(90.0), Color::Red);
    }

    #[test]
    fn icon_mode_replaces_text_labels() {
        assert_eq!(metric_name(Metric::Cpu, false), "cpu");
        assert_eq!(metric_name(Metric::Cpu, true), "▣");
        assert_eq!(metric_name(Metric::Network, true), "↕");
    }
}
