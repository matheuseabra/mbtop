//! Terminal lifecycle and event loop for the `mbtop` binary.

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use mbtop::app::App;
use mbtop::cli::{self, Action, Config};
use mbtop::ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match cli::parse(std::env::args().skip(1))? {
        Action::Help => println!("{}", cli::help_text()),
        Action::Version => println!("{}", cli::version_text()),
        Action::Run(config) => run(config)?,
    }
    Ok(())
}

fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let interval = config.interval;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    let _terminal = TerminalGuard;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let mut app = App::new(config);
    app.sample();
    terminal.draw(|frame| ui::render(frame, &app.stats()))?;
    let mut next_sample = Instant::now() + interval;

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
                    terminal.draw(|frame| ui::render(frame, &app.stats()))?;
                }
                _ => {}
            }
        }
        if Instant::now() >= next_sample {
            app.sample();
            terminal.draw(|frame| ui::render(frame, &app.stats()))?;
            next_sample = Instant::now() + interval;
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
