use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Button, CheckButton, ComboBoxText,
    Box as GtkBox, Orientation,};
use std::cell::RefCell;
use std::rc::Rc;

use crate::devices::{list_camera_devices, list_screens};
use crate::recording::{start_recording, stop_recording};

pub fn build_ui(app: &Application) {
    // Create the main application window.
    let window = ApplicationWindow::builder()
    .application(app)
    .title("Rust OBS for Wayland")
    .default_width(500)
    .default_height(300)
    .build();

    // Apply a dark CSS theme inspired by Discord.
    let css = "
    window {
    background-color: #36393F;
    color: #DCDDDE;
    font-family: 'Segoe UI', sans-serif;
    font-size: 14px;
}
button {
background-color: #7289DA;
color: white;
border-radius: 4px;
padding: 6px 10px;
}
button:hover {
background-color: #677BC4;
}
checkbutton, label {
margin: 5px;
}
comboboxtext {
margin: 5px;
padding: 4px;
}
";
// Assuming `css` is a &str containing your CSS data:
let provider = gtk4::CssProvider::new();
provider.load_from_data(css); // load_from_data now takes a &str and returns ()

if let Some(display) = gtk4::gdk::Display::default() {
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}


// Build a vertical container for layout.
let vbox = GtkBox::new(Orientation::Vertical, 10);
vbox.set_margin_top(10);
vbox.set_margin_bottom(10);
vbox.set_margin_start(10);
vbox.set_margin_end(10);

// Checkbuttons to select recording options.
let screen_checkbox = CheckButton::with_label("Record Screen");
screen_checkbox.set_active(true);
let camera_checkbox = CheckButton::with_label("Record Camera");

// ComboBoxText for selecting a screen (if multiple are available).
let screen_combo = ComboBoxText::new();
for screen in list_screens() {
    screen_combo.append_text(&screen);
}
if screen_combo.active_text().is_none() {
    screen_combo.append_text("WAYLAND_DISPLAY");
    screen_combo.set_active(Some(0));
} else {
    screen_combo.set_active(Some(0));
}

// ComboBoxText for selecting a camera device.
let camera_combo = ComboBoxText::new();
for device in list_camera_devices() {
    camera_combo.append_text(&device);
}
// Set the first camera device as the default, if available.
camera_combo.set_active(Some(0));

// Buttons to start and stop recording.
let start_button = Button::with_label("Start Recording");
let stop_button = Button::with_label("Stop Recording");
stop_button.set_sensitive(false);

// Add all widgets to the container.
vbox.append(&screen_checkbox);
vbox.append(&screen_combo);
vbox.append(&camera_checkbox);
vbox.append(&camera_combo);
vbox.append(&start_button);
vbox.append(&stop_button);

window.set_child(Some(&vbox));
window.show();

// Shared state to hold the handle of the ffmpeg process.
let recording_handle = Rc::new(RefCell::new(None));

// Clone state and widgets for the start button callback.
let recording_handle_clone = recording_handle.clone();
let start_button_clone = start_button.clone();
let stop_button_clone = stop_button.clone();
let screen_checkbox_clone = screen_checkbox.clone();
let camera_checkbox_clone = camera_checkbox.clone();
let screen_combo_clone = screen_combo.clone();
let camera_combo_clone = camera_combo.clone();

start_button.connect_clicked(move |_| {
    // Capture the user's selection.
    let record_screen = screen_checkbox_clone.is_active();
    let record_camera = camera_checkbox_clone.is_active();
    let screen_choice = screen_combo_clone.active_text().unwrap_or_else(|| "WAYLAND_DISPLAY".into());
    let camera_choice = camera_combo_clone.active_text().unwrap_or_else(|| "/dev/video0".into());

    // Start the recording based on selected options.
    match start_recording(record_screen, record_camera, &screen_choice, &camera_choice) {
        Ok(child) => {
            *recording_handle_clone.borrow_mut() = Some(child);
            start_button_clone.set_sensitive(false);
            stop_button_clone.set_sensitive(true);
        },
        Err(e) => {
            eprintln!("Error starting recording: {}", e);
        }
    }
});

// Clone state for the stop button callback.
let recording_handle_clone2 = recording_handle.clone();
let start_button_clone2 = start_button.clone();
let stop_button_clone2 = stop_button.clone();

stop_button.connect_clicked(move |_| {
    if let Err(e) = stop_recording(&mut recording_handle_clone2.borrow_mut()) {
        eprintln!("Error stopping recording: {}", e);
    }
    start_button_clone2.set_sensitive(true);
    stop_button_clone2.set_sensitive(false);
});
}
