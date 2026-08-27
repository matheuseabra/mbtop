# Terminal UI research

## Decision

Use Ratatui on top of the existing Crossterm and Sysinfo stack. Ratatui's built-in `Sparkline`, `LineGauge`, rounded `Block`, and layout primitives cover the dashboard without adding a separate charting dependency.

## What informed the design

- [btop's README](https://github.com/aristocratos/btop#configurability) describes separate graph symbol modes (`braille`, `block`, and `tty`), rounded boxes, terminal synchronization, and configurable update intervals. mbtop adopts the compact part of that model: a rounded box, a short block sparkline, and redraws on resize.
- [Ratatui's Sparkline](https://docs.rs/ratatui/latest/ratatui/widgets/struct.Sparkline.html) supports bounded data, configurable bar symbols, and per-widget styling. It is a good fit for CPU history in one terminal row.
- [Ratatui's LineGauge](https://docs.rs/ratatui/latest/ratatui/widgets/struct.LineGauge.html) is explicitly designed as a compact one-line gauge, so memory and disk use it instead of a larger multi-row bar.
- [Ratatui's Block](https://docs.rs/ratatui/latest/ratatui/widgets/struct.Block.html) provides rounded borders and calculates inner space for nested layouts, which makes resize handling less brittle than manual ANSI padding.
- [Crossterm's style API](https://docs.rs/crossterm/latest/crossterm/style/index.html) supports resetting to the terminal's default foreground and using attributes. mbtop therefore uses the terminal default for all text, including muted content, while retaining dim and bold attributes where useful.
- The chart gradient also stays terminal-native: it uses named ANSI palette entries (`Cyan`, `Green`, `Yellow`, and `Red`) per data point instead of fixed RGB values. That gives a visible low-to-high gradient while allowing terminal themes to control the actual colors.

## Compact layout

The default layout is intentionally five content rows inside one rounded block:

1. CPU percentage plus a short sparkline.
2. Memory line gauge.
3. Disk line gauge.
4. Load averages and network rate side by side.
5. Uptime, CPU count, and quit hint.

The minimum supported outer area is `36×8`. Below either dimension, the app renders a centered size message and redraws immediately on `Event::Resize`.
