use eframe::egui;
use std::sync::mpsc::Sender;
use std::path::PathBuf;
use rfd::FileDialog;
use log::{info, error};

pub enum Command {
    StartRecording(PathBuf),
    StopRecording,
}

pub struct RecorderApp {
    sender: Sender<Command>,
    recording: bool,
}

impl RecorderApp {
    pub fn new(sender: Sender<Command>) -> Self {
        Self {
            sender,
            recording: false,
        }
    }

    pub fn apply_discord_theme(&mut self, ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(54, 57, 63);  // Discord #36393f
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 221, 222)); // #dcddde
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(67, 71, 77);       // Slightly lighter gray
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(88, 101, 242);       // Discord accent #5865f2
        ctx.set_visuals(visuals);
    }
}

impl eframe::App for RecorderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Recorder");
                ui.add_space(20.0);

                if !self.recording {
                    if ui.button("Start Recording").clicked() {
                        if let Some(path) = FileDialog::new()
                            .add_filter("MP4 Video", &["mp4"])
                            .set_file_name("recording.mp4")
                            .save_file()
                            {
                                if let Err(e) = self.sender.send(Command::StartRecording(path)) {
                                    error!("Failed to send start command: {}", e);
                                } else {
                                    self.recording = true;
                                    info!("Recording started");
                                }
                            }
                    }
                } else {
                    if ui.button("Stop Recording").clicked() {
                        if let Err(e) = self.sender.send(Command::StopRecording) {
                            error!("Failed to send stop command: {}", e);
                        } else {
                            self.recording = false;
                            info!("Recording stopped");
                        }
                    }
                }

                ui.add_space(10.0);
                ui.label(if self.recording { "Status: Recording" } else { "Status: Idle" });
            });
        });
    }
}
