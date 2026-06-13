//! Android-specific code for Grafito

use android_activity::AndroidApp;
use winit::event_loop::EventLoop;
use winit::platform::android::EventLoopBuilderExtAndroid;

#[no_mangle]
pub extern "C" fn android_main(app: AndroidApp) {
    // Initialize Android logger
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Grafito"),
    );

    log::info!("Starting Grafito on Android");

    // Create event loop with Android context
    let event_loop = EventLoop::builder()
        .with_android_app(app)
        .build()
        .expect("Failed to create event loop");

    // Run the app using eframe
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([800.0, 600.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Grafito CAD",
        options,
        Box::new(|cc| {
            // Configure touch UI
            configure_touch_ui(&cc.egui_ctx);
            Ok(Box::new(crate::GrafitoApp::new(cc)))
        }),
    )
    .expect("Failed to run Grafito");
}

fn configure_touch_ui(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Increase touch target sizes
    style.spacing.interact_size = egui::vec2(44.0, 44.0);
    style.spacing.button_padding = egui::vec2(16.0, 12.0);
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);

    // Larger fonts for mobile
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(18.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(18.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(14.0));

    ctx.set_style(style);

    // Configure for touch
    ctx.options_mut(|opt| {
        opt.zoom_with_keyboard = false;
    });
}
