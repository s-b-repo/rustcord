// src/gui.rs
use crate::run_recording;
use anyhow::Result;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub struct RecorderApp {
    is_recording: bool,
    stop_flag: Arc<AtomicBool>,
    recording_handle: Option<thread::JoinHandle<()>>,
    status_message: String,
}

impl RecorderApp {
    pub fn new() -> Self {
        Self {
            is_recording: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
            recording_handle: None,
            status_message: "Idle".to_owned(),
        }
    }
}

impl eframe::App for RecorderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("OBS Recorder - Modern GUI");
            ui.label(&self.status_message);

            // Start recording if not already recording.
            if !self.is_recording {
                if ui.button("Start Recording").clicked() {
                    self.status_message = "Recording started...".to_owned();
                    self.is_recording = true;
                    self.stop_flag.store(false, Ordering::SeqCst);
                    let stop_flag_clone = self.stop_flag.clone();
                    // Spawn the recording thread.
                    self.recording_handle = Some(thread::spawn(move || {
                        if let Err(e) = run_recording(stop_flag_clone) {
                            eprintln!("Recording error: {:?}", e);
                        }
                    }));
                }
            } else {
                // Provide a button to stop recording.
                if ui.button("Stop Recording").clicked() {
                    self.status_message = "Stopping recording...".to_owned();
                    self.stop_flag.store(true, Ordering::SeqCst);
                    if let Some(handle) = self.recording_handle.take() {
                        let _ = handle.join();
                    }
                    self.is_recording = false;
                    self.status_message =
                        "Recording stopped. Output saved to output.mp4".to_owned();
                }
            }

            ui.separator();
            ui.label("Overlay Configuration (Dynamic changes simulated automatically)");
            ui.label("Additional advanced controls could be added here.");
        });
    }
}
