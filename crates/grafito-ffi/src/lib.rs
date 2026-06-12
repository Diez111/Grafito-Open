//! Grafito FFI — Bridge para Android (Jetpack Compose + UniFFI)

uniffi::setup_scaffolding!();

pub mod dto;
pub mod converters;
pub mod command_processor;
pub mod bridge;
pub mod canvas;
pub mod persist;
pub mod jni;

pub use bridge::GrafitoEngine;
pub use canvas::CanvasRenderer;
