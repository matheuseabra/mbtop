//! Sampler: owns the system handles and turns them into plain [`Stats`]
//! the UI can render without knowing about `sysinfo`.

use std::collections::VecDeque;
use std::time::Instant;

use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind, System};

use crate::cli::Config;
use crate::metrics::{self, DiskUsage, NetworkUsage};

/// One render's worth of measurements, borrowed from the sampler.
///
/// Rendering is a pure function of this snapshot, so the UI can be exercised
/// with fabricated values instead of a live system.
#[derive(Debug, Clone, Copy)]
pub struct Stats<'a> {
    /// Host name shown in the card title.
    pub host: &'a str,
    /// Global CPU usage percentage.
    pub cpu: f32,
    /// Recent CPU samples, oldest first.
    pub cpu_history: &'a VecDeque<f32>,
    /// Logical core count.
    pub cores: usize,
    /// Used memory in bytes.
    pub memory_used: u64,
    /// Total memory in bytes.
    pub memory_total: u64,
    /// Selected disk usage, or `None` when no disk matches.
    pub disk: Option<DiskUsage>,
    /// 1, 5, and 15 minute load averages.
    pub load: (f64, f64, f64),
    /// Network rates in bytes per second.
    pub network: NetworkUsage,
    /// Seconds the system has been up.
    pub uptime_seconds: u64,
    /// Use compact Unicode glyphs instead of text labels.
    pub icons: bool,
    /// Omit the dashboard's outer border.
    pub borderless: bool,
}

/// Owns the system handles and derived state between samples.
pub struct App {
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
    /// Creates a sampler with the first sample not yet taken.
    pub fn new(config: Config) -> Self {
        let refresh = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything());

        Self {
            config,
            host: System::host_name().unwrap_or_else(|| "local".to_string()),
            system: System::new_with_specifics(refresh),
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            cpu_history: VecDeque::with_capacity(metrics::HISTORY_LEN),
            last_network: NetworkUsage::default(),
            network_rate: NetworkUsage::default(),
            last_sample: None,
        }
    }

    /// Refreshes every source and records one sample.
    pub fn sample(&mut self) {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.disks.refresh(true);
        self.networks.refresh(true);

        metrics::push_history(&mut self.cpu_history, self.system.global_cpu_usage());

        let network = metrics::network_totals(
            self.networks
                .iter()
                .map(|(name, data)| (name.as_str(), data.received(), data.transmitted())),
        );
        if let Some(previous) = self.last_sample {
            let elapsed = previous.elapsed().as_secs_f64();
            self.network_rate = NetworkUsage {
                received: metrics::rate(
                    network.received.saturating_sub(self.last_network.received),
                    elapsed,
                ),
                transmitted: metrics::rate(
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

    /// Borrows the current measurements for rendering.
    pub fn stats(&self) -> Stats<'_> {
        let load = System::load_average();
        Stats {
            host: &self.host,
            cpu: self.system.global_cpu_usage(),
            cpu_history: &self.cpu_history,
            cores: self.system.cpus().len(),
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            disk: metrics::disk_usage(
                self.disks.iter().map(|disk| {
                    (
                        disk.mount_point(),
                        disk.total_space(),
                        disk.available_space(),
                    )
                }),
                self.config.disk_path.as_deref(),
            ),
            load: (load.one, load.five, load.fifteen),
            network: self.network_rate,
            uptime_seconds: System::uptime(),
            icons: self.config.icons,
            borderless: self.config.borderless,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::cli::DEFAULT_INTERVAL;

    fn config() -> Config {
        Config {
            disk_path: None,
            interval: DEFAULT_INTERVAL,
            icons: false,
            borderless: false,
        }
    }

    #[test]
    fn first_sample_seeds_history_without_a_network_rate() {
        let mut app = App::new(config());
        app.sample();

        assert_eq!(app.cpu_history.len(), 1);
        assert_eq!(app.network_rate, NetworkUsage::default());
    }

    #[test]
    fn stats_reports_live_system_values() {
        let mut app = App::new(config());
        app.sample();

        let stats = app.stats();
        assert_eq!(stats.cpu_history.len(), 1);
        assert!(stats.cores >= 1);
        assert!(stats.memory_total > 0);
        assert!(!stats.host.is_empty());
    }

    #[test]
    fn stats_honors_a_requested_disk_path() {
        let config = Config {
            disk_path: Some(std::path::PathBuf::from("/")),
            interval: Duration::from_secs(1),
            icons: false,
            borderless: false,
        };
        let mut app = App::new(config);
        app.sample();

        assert!(app.stats().disk.is_some());
    }
}
