//! Pure sampling math and value formatting shared by the sampler and the UI.
//!
//! Everything here is a plain function over numbers, strings, and tuples so
//! behavior can be tested without touching live system state.

use std::collections::VecDeque;
use std::path::Path;

/// Number of CPU samples kept for the sparkline.
pub const HISTORY_LEN: usize = 28;

/// Aggregate receive/transmit byte counters or rates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NetworkUsage {
    /// Bytes received per second, or in total, depending on the producer.
    pub received: u64,
    /// Bytes transmitted per second, or in total, depending on the producer.
    pub transmitted: u64,
}

/// Space usage for one disk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiskUsage {
    /// Total capacity in bytes.
    pub total: u64,
    /// Used capacity in bytes.
    pub used: u64,
    /// Used fraction as a percentage.
    pub percent: f32,
}

/// Appends a CPU sample, keeping at most [`HISTORY_LEN`] entries.
pub fn push_history(history: &mut VecDeque<f32>, value: f32) {
    if history.len() == HISTORY_LEN {
        history.pop_front();
    }
    history.push_back(value);
}

/// Formats a byte count with compact binary units, e.g. `12.9G`.
///
/// ```
/// use mbtop::metrics::bytes;
///
/// assert_eq!(bytes(5 * 1024 * 1024), "5.0M");
/// ```
pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = value as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0}{unit}", unit = UNITS[unit])
    } else {
        format!("{value:.1}{unit}", unit = UNITS[unit])
    }
}

/// Used fraction of `total` as a percentage; `0.0` when `total` is zero.
pub fn percentage(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0) as f32
    }
}

/// Bytes moved per second over `seconds`, rounded to whole bytes.
///
/// Intervals shorter than a millisecond are treated as one millisecond so a
/// zero-length interval cannot divide by zero.
pub fn rate(bytes: u64, seconds: f64) -> u64 {
    (bytes as f64 / seconds.max(0.001)).round() as u64
}

/// Formats an uptime as `4d 00h 05m`, `4h 05m`, or `5m`.
pub fn format_uptime(seconds: u64) -> String {
    let mut seconds = seconds;
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

/// Picks the disk to report from `(mount point, total, available)` tuples.
///
/// With a `requested` path, prefers an exact mount-point match, then any mount
/// containing the path. Without one, prefers the root mount, then the first
/// disk. The tuple list mirrors mount-table order, which the fallback rules
/// depend on; the number of mounts is tiny.
pub fn disk_usage<'a, I>(disks: I, requested: Option<&Path>) -> Option<DiskUsage>
where
    I: IntoIterator<Item = (&'a Path, u64, u64)>,
{
    let disks: Vec<(&Path, u64, u64)> = disks.into_iter().collect();
    let &(_, total, available) = match requested {
        Some(path) => disks
            .iter()
            .find(|(mount, _, _)| *mount == path)
            .or_else(|| {
                disks
                    .iter()
                    .filter(|(mount, _, _)| path.starts_with(*mount))
                    .max_by_key(|(mount, _, _)| mount.components().count())
            }),
        None => disks
            .iter()
            .find(|(mount, _, _)| *mount == Path::new("/"))
            .or_else(|| disks.first()),
    }?;

    let used = total.saturating_sub(available);
    Some(DiskUsage {
        total,
        used,
        percent: percentage(used, total),
    })
}

/// Sums receive/transmit counters across interfaces, skipping loopback.
pub fn network_totals<'a, I>(interfaces: I) -> NetworkUsage
where
    I: IntoIterator<Item = (&'a str, u64, u64)>,
{
    interfaces
        .into_iter()
        .filter(|(name, _, _)| !name.starts_with("lo"))
        .fold(NetworkUsage::default(), |total, (_, received, sent)| {
            NetworkUsage {
                received: total.received.saturating_add(received),
                transmitted: total.transmitted.saturating_add(sent),
            }
        })
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn bytes_formats_zero_as_bytes() {
        assert_eq!(bytes(0), "0B");
    }

    #[test]
    fn bytes_formats_binary_units_compactly() {
        assert_eq!(bytes(1023), "1023B");
        assert_eq!(bytes(1024), "1.0K");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0M");
    }

    #[test]
    fn bytes_stops_at_the_largest_unit() {
        let terabytes = 4 * 1024u64 * 1024 * 1024 * 1024;
        assert_eq!(bytes(terabytes + 100), "4.0T");
        assert_eq!(bytes(u64::MAX), "16777216.0T");
    }

    #[test]
    fn percentage_returns_zero_for_empty_metrics() {
        assert_eq!(percentage(1, 0), 0.0);
    }

    #[test]
    fn percentage_returns_used_ratio() {
        assert_eq!(percentage(25, 100), 25.0);
    }

    #[test]
    fn rate_rounds_to_whole_bytes_per_second() {
        assert_eq!(rate(1500, 2.0), 750);
        assert_eq!(rate(1, 2.0), 1);
    }

    #[test]
    fn rate_treats_sub_millisecond_intervals_as_one_millisecond() {
        assert_eq!(rate(0, 0.0), 0);
        assert_eq!(rate(10, 0.0), 10_000);
    }

    #[test]
    fn uptime_formats_minutes_only_below_an_hour() {
        assert_eq!(format_uptime(5 * 60), "5m");
        assert_eq!(format_uptime(0), "0m");
    }

    #[test]
    fn uptime_pads_minutes_below_a_day() {
        assert_eq!(format_uptime(4 * 3_600 + 5 * 60), "4h 05m");
    }

    #[test]
    fn uptime_includes_days_when_present() {
        assert_eq!(format_uptime(4 * 86_400 + 5 * 3_600), "4d 05h 00m");
    }

    #[test]
    fn history_keeps_only_the_latest_samples() {
        let mut history = VecDeque::new();
        for value in 0..(HISTORY_LEN + 5) {
            push_history(&mut history, value as f32);
        }
        assert_eq!(history.len(), HISTORY_LEN);
        assert_eq!(history.front(), Some(&(5.0)));
        assert_eq!(history.back(), Some(&(HISTORY_LEN as f32 + 4.0)));
    }

    #[test]
    fn disk_usage_matches_requested_mount_exactly() {
        let root = Path::new("/");
        let disks = [(root, 100, 25), (Path::new("/tmp"), 50, 10)];
        let usage = disk_usage(disks, Some(Path::new("/"))).unwrap();
        assert_eq!(usage.total, 100);
        assert_eq!(usage.used, 75);
        assert_eq!(usage.percent, 75.0);
    }

    #[test]
    fn disk_usage_falls_back_to_a_containing_mount() {
        let disks = [(Path::new("/"), 100, 25), (Path::new("/tmp"), 50, 10)];
        let usage = disk_usage(disks, Some(Path::new("/tmp/file"))).unwrap();
        assert_eq!(usage.total, 50);
    }

    #[test]
    fn disk_usage_defaults_to_root_then_first_disk() {
        let disks = [(Path::new("/"), 100, 25), (Path::new("/tmp"), 50, 10)];
        assert_eq!(disk_usage(disks, None).unwrap().total, 100);

        let no_root = [(Path::new("/Volumes/X"), 50, 10)];
        assert_eq!(disk_usage(no_root, None).unwrap().total, 50);
        assert!(disk_usage([], None).is_none());
    }

    #[test]
    fn disk_usage_reports_zero_percent_for_empty_disks() {
        let disks = [(Path::new("/"), 0, 0)];
        assert_eq!(disk_usage(disks, None).unwrap().percent, 0.0);
    }

    #[test]
    fn network_totals_skips_loopback_and_sums_the_rest() {
        let interfaces = [("lo0", 1_000, 1_000), ("en0", 300, 200), ("utun0", 50, 40)];
        let totals = network_totals(interfaces);
        assert_eq!(totals.received, 350);
        assert_eq!(totals.transmitted, 240);
    }

    #[test]
    fn network_totals_saturates_instead_of_overflowing() {
        let interfaces = [("en0", u64::MAX, u64::MAX), ("en1", 10, 10)];
        let totals = network_totals(interfaces);
        assert_eq!(totals.received, u64::MAX);
        assert_eq!(totals.transmitted, u64::MAX);
    }
}
