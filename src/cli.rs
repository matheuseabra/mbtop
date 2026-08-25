//! Command-line interface: argument parsing, help text, and the [`Config`]
//! the dashboard runs with.

use std::path::PathBuf;
use std::time::Duration;

/// Version reported by `--version` and the help text.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Refresh interval used when `--interval` is not given.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);

/// What the process should do after parsing arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Run the dashboard with the parsed configuration.
    Run(Config),
    /// Print the help text and exit successfully.
    Help,
    /// Print the version and exit successfully.
    Version,
}

/// Runtime configuration for the dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Disk mount or path to report; `None` selects the root disk.
    pub disk_path: Option<PathBuf>,
    /// Time between samples.
    pub interval: Duration,
    /// Use compact Unicode glyphs instead of text labels.
    pub icons: bool,
}

/// Parses command-line arguments, without the program name.
///
/// Parsing is pure: printing is left to the caller, which receives an
/// [`Action`] instead.
///
/// # Examples
///
/// ```
/// use mbtop::cli::{parse, Action, Config, DEFAULT_INTERVAL};
///
/// let action = parse([]).unwrap();
/// assert_eq!(
///     action,
///     Action::Run(Config {
///         disk_path: None,
///         interval: DEFAULT_INTERVAL,
///         icons: false,
///     })
/// );
/// ```
///
/// # Errors
///
/// Returns a human-readable message for missing flag values, a zero or
/// non-numeric interval, and unknown arguments.
pub fn parse<I>(args: I) -> Result<Action, String>
where
    I: IntoIterator<Item = String>,
{
    let mut disk_path = None;
    let mut interval = DEFAULT_INTERVAL;
    let mut icons = false;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
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

    Ok(Action::Run(Config {
        disk_path,
        interval,
        icons,
    }))
}

/// Full `--help` text.
pub fn help_text() -> String {
    format!(
        "mbtop {VERSION} — tiny system monitor for terminal dashboard panes\n\n\
Usage: mbtop [OPTIONS]\n\n\
Options:\n\
  -d, --disk <PATH>       Disk mount or path to report (default: /)\n\
  -i, --interval <MS>     Refresh interval in milliseconds (default: 1000)\n\
      --icon              Use compact Unicode glyphs instead of text labels\n\
  -h, --help              Show this help\n\
  -V, --version           Show version\n\n\
Keys: q / Esc / Ctrl-C to quit"
    )
}

/// `--version` line.
pub fn version_text() -> String {
    format!("mbtop {VERSION}")
}
