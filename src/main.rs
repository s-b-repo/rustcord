mod gui;
mod core;
mod devices;
mod extensions;

use gio::ApplicationFlags;
use libadwaita::prelude::*;
use libadwaita::Application;
use anyhow::Result;
use log::info;

fn main() -> Result<()> {
    // Initialize logger for runtime diagnostics
    env_logger::init();
    info!("Launching Waycord Recorder with all advanced features...");

    // Build our main pipeline with advanced modules (scene switcher, overlays, streaming, plugin system, etc.)
    core::encoder::init_pipeline_with_advanced_features()?;

    // Create the libadwaita-based GTK application
    let app = Application::new(
        Some("com.waycord.recorder.ultimate"),
        ApplicationFlags::FLAGS_NONE,
    );

    // Build the GUI on activation
    app.connect_activate(|app| {
        gui::window::build_ui(app);
    });

    // Run the main GTK loop
    app.run();

    Ok(())
}
