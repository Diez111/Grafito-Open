//! Grafito Desktop Application — Main entry point

#![allow(
    clippy::needless_range_loop,
    clippy::if_same_then_else,
    clippy::manual_clamp
)]

fn main() {
    if let Err(e) = grafito_app::run_app() {
        log::error!("Failed to run Grafito: {}", e);
        std::process::exit(1);
    }
}
