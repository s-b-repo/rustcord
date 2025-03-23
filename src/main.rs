mod gui;
mod recording;
mod devices;

use gtk4::prelude::*;
use gtk4::Application;

fn main() {
    // Create the GTK application with a unique application ID.
    let app = Application::builder()
    .application_id("Rustcord")
    .build();

    // When the application activates, build the UI.
    app.connect_activate(gui::build_ui);
    app.run();
}
