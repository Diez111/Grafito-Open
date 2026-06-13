//! Grafito FFI — Bridge para Android (Jetpack Compose + UniFFI)

uniffi::setup_scaffolding!();

pub mod bridge;
pub mod canvas;
pub mod command_processor;
pub mod converters;
pub mod dto;
pub mod jni;
pub mod persist;

pub use bridge::GrafitoEngine;
pub use canvas::CanvasRenderer;
