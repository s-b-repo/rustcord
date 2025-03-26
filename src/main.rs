// main.rs
mod gui;
mod recording;
mod devices;

use gtk4::prelude::*;
use gtk4::Application;
use gstreamer as gst;

fn main() {
    // Initialize GTK application with a unique ID and GStreamer for media handling.
    let app = Application::builder()
        .application_id("com.example.RustWaylandRecorder")
        .build();

    // Initialize GStreamer (needed for pipeline setup).
    gst::init().expect("Failed to initialize GStreamer");

    // Activate the GUI when the application starts.
    app.connect_activate(gui::build_ui);
    app.run();
}
