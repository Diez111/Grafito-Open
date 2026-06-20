//! Command parsing and execution.
//!
//! This module re-exports the grafito_command interpreter so the desktop app
//! has a single local entry point for parsing and executing user commands.

pub use grafito_command::commands::{parse_point_str, parse_preview, process_input};
