use std::sync::mpsc::{channel, Sender, Receiver};
use std::path::PathBuf;
use std::thread;
use eframe::egui;
use log::info;
use env_logger;

mod gui;
mod recorder;

use gui::RecorderApp;
use recorder::{Recorder, Command};

fn main() -> anyhow::Result<()> {
    env_logger::init(); // Initialize logging
    info!("Starting Recorder Application");

    // Create communication channels
    let (tx, rx): (Sender<Command>, Receiver<Command>) = channel();

    // Spawn worker thread for recording
    let recorder = Recorder::new(rx);
    thread::spawn(move || {
        recorder.run();
    });

    // Set up egui with Discord theming
    let native_options = eframe::NativeOptions {
        winit: winit::window::WindowAttributes::default()
        .with_title("Recorder"),
        ..Default::default()
    };

    eframe::run_native(
        "Recorder",
        native_options,
        Box::new(|cc| {
            let mut app = RecorderApp::new(tx);
            app.apply_discord_theme(&cc.egui_ctx);
            Box::new(app)
        }),
    )?;

    Ok(())
}
