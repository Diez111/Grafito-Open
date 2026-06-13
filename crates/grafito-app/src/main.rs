//! Grafito Desktop Application — Main entry point

#![allow(
    clippy::needless_range_loop,
    clippy::if_same_then_else,
    clippy::manual_clamp
)]

fn main() {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_decorations(true)
            .with_transparent(false),
        multisampling: grafito_app::MSAA_SAMPLES,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "Grafito",
        options,
        Box::new(|cc| Ok(Box::new(grafito_app::GrafitoApp::new(cc)))),
    ) {
        log::error!("Failed to run Grafito: {}", e);
        std::process::exit(1);
    }
}
