//! `mbtop` is a tiny system monitor for terminal dashboard panes.
//!
//! The crate ships as a library plus a thin binary so each concern can be
//! tested through its own seam:
//!
//! - [`cli`] parses arguments into a [`cli::Config`] or a print-and-exit action
//! - [`metrics`] holds the pure sampling math and value formatting
//! - [`app`] samples the system and produces an [`app::Stats`] snapshot
//! - [`ui`] renders a [`app::Stats`] snapshot with Ratatui

pub mod app;
pub mod cli;
pub mod metrics;
pub mod ui;
