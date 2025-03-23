use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Button, CheckButton, ComboBoxText, Entry, FileChooserAction,
    FileChooserNative, Fixed, Label, Orientation, ResponseType,
};
use gtk4::gdk::Display;
use gtk4::GestureDrag;
use std::cell::RefCell;
#[allow(unused_imports)]
use std::path::Path;
use std::rc::Rc;
use glib::clone;
use crate::devices::{list_camera_devices, list_screens};
use crate::recording::{start_recording, stop_recording};
use gtk::{ApplicationWindow, FileChooserAction, FileChooserNative};

pub fn build_ui(app: &Application) {
    // Create the main application window.
    let window = ApplicationWindow::builder()
    .application(app)
    .title("Rust OBS for Wayland")
    .default_width(600)
    .default_height(700)
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
comboboxtext, entry {
margin: 5px;
padding: 4px;
}
";
let provider = gtk4::CssProvider::new();
provider.load_from_data(css);
if let Some(display) = Display::default() {
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

// Build a vertical container for layout.
let vbox = gtk4::Box::new(Orientation::Vertical, 10);
vbox.set_margin_top(10);
vbox.set_margin_bottom(10);
vbox.set_margin_start(10);
vbox.set_margin_end(10);

// --- Recording Options Section ---

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
camera_combo.set_active(Some(0));

// --- Output Folder & File Name Section ---

let output_folder_label = Label::new(Some("Output Folder:"));
let chosen_folder_label = Label::new(Some("Not Selected"));
let select_folder_button = Button::with_label("Select Folder");

// When clicked, open a folder chooser dialog.
select_folder_button.connect_clicked(clone!(@weak window, @weak chosen_folder_label => move |_| {
    let folder_chooser = FileChooserNative::new(
        Some("Select Output Folder"),
                                                Some(&window),
                                                FileChooserAction::SelectFolder,
                                                Some("Select"),
                                                Some("Cancel"),
    );
    folder_chooser.connect_response(clone!(@weak chosen_folder_label => move |dialog, response| {
        if response == ResponseType::Accept {
            if let Some(folder) = dialog.file() {
                if let Some(path) = folder.path() {
                    chosen_folder_label.set_text(path.to_str().unwrap_or("Invalid Path"));
                }
            }
        }
        dialog.close();
    }));
    folder_chooser.show();
}));

let file_name_label = Label::new(Some("Output File Name:"));
let file_name_entry = Entry::new();
file_name_entry.set_placeholder_text(Some("e.g., screen_recording.mp4"));
file_name_entry.set_text("screen_recording.mp4");

// --- Recording Settings: Resolution and FPS ---

let resolution_label = Label::new(Some("Resolution:"));
let resolution_combo = ComboBoxText::new();
resolution_combo.append_text("1920x1080");
resolution_combo.append_text("1280x720");
resolution_combo.append_text("640x480");
resolution_combo.set_active(Some(0));

let fps_label = Label::new(Some("FPS:"));
let fps_combo = ComboBoxText::new();
fps_combo.append_text("30");
fps_combo.append_text("60");
fps_combo.append_text("24");
fps_combo.set_active(Some(0));

// --- Preview Area ---

let preview_label = Label::new(Some("Preview:"));
// Using a Fixed container to allow absolute positioning of the camera preview.
let preview_area = Fixed::new();
preview_area.set_size_request(600, 350);

// Background for screen preview (a simple label here; in a real app, this might be a video widget).
let screen_preview = Label::new(Some("Screen Preview"));
screen_preview.set_margin_top(0);
screen_preview.set_margin_start(0);
// Fill the area.
preview_area.put(&screen_preview, 0.0, 0.0);
screen_preview.set_size_request(600, 350);

// Create the camera preview widget.
let camera_preview = Label::new(Some("Camera Preview"));
camera_preview.set_widget_name("camera_preview");
// Give it a fixed size (for example, 160x120).
camera_preview.set_size_request(160, 120);
// Initially position it in the top-left corner.
preview_area.put(&camera_preview, 10.0, 10.0);

// Add drag gesture to the camera preview widget.
// Create the GestureDrag
let drag = GestureDrag::new(); // No argument needed
// Attach the gesture to the widget
camera_preview.add_controller(&drag);
// Store the initial offset.
let initial_offset = Rc::new(RefCell::new((10.0, 10.0)));
// Closure to update the widget's position.
{
    let preview_area_clone = preview_area.clone();
    let initial_offset = initial_offset.clone();
    drag.connect_drag_update(move |_gesture, offset_x, offset_y| {
        // Calculate new position based on the drag offset.
        let (init_x, init_y) = *initial_offset.borrow();
        let new_x = init_x + offset_x;
        let new_y = init_y + offset_y;
        preview_area_clone.move_(&camera_preview, new_x as f64, new_y as f64);

    });
    // On drag end, update the initial offset.
    let initial_offset = initial_offset.clone();
    drag.connect_drag_end(move |_gesture, offset_x, offset_y| {
        let (init_x, init_y) = *initial_offset.borrow();
        let new_x = init_x + offset_x;
        let new_y = init_y + offset_y;
        *initial_offset.borrow_mut() = (new_x, new_y);
    });
}

// --- Start and Stop Buttons ---
let start_button = Button::with_label("Start Recording");
let stop_button = Button::with_label("Stop Recording");
stop_button.set_sensitive(false);

// Add all widgets to the container.
vbox.append(&screen_checkbox);
vbox.append(&screen_combo);
vbox.append(&camera_checkbox);
vbox.append(&camera_combo);
vbox.append(&output_folder_label);
vbox.append(&select_folder_button);
vbox.append(&chosen_folder_label);
vbox.append(&file_name_label);
vbox.append(&file_name_entry);
vbox.append(&resolution_label);
vbox.append(&resolution_combo);
vbox.append(&fps_label);
vbox.append(&fps_combo);
vbox.append(&preview_label);
vbox.append(&preview_area);
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
let file_name_entry_clone = file_name_entry.clone();
let chosen_folder_label_clone = chosen_folder_label.clone();
let resolution_combo_clone = resolution_combo.clone();
let fps_combo_clone = fps_combo.clone();

start_button.connect_clicked(move |_| {
    // Capture the user's selection.
    let record_screen = screen_checkbox_clone.is_active();
    let record_camera = camera_checkbox_clone.is_active();
    let screen_choice = screen_combo_clone
    .active_text()
    .unwrap_or_else(|| "WAYLAND_DISPLAY".into());
    let camera_choice = camera_combo_clone
    .active_text()
    .unwrap_or_else(|| "/dev/video0".into());
    let file_name = file_name_entry_clone.text().to_string();

    // Combine the chosen output folder and file name.
    let folder = chosen_folder_label_clone.text();
    let full_path = if folder == "Not Selected" || folder.is_empty() {
        // Use current directory if no folder selected.
        file_name.clone()
    } else {
        let mut path = folder.to_string();
        if !path.ends_with(std::path::MAIN_SEPARATOR) {
            path.push(std::path::MAIN_SEPARATOR);
        }
        path.push_str(&file_name);
        path
    };

    let resolution = resolution_combo_clone
    .active_text()
    .unwrap_or_else(|| "1920x1080".into());
    let fps = fps_combo_clone.active_text().unwrap_or_else(|| "30".into());

    // Start the recording based on selected options.
    match start_recording(
        record_screen,
        record_camera,
        &screen_choice,
        &camera_choice,
        &resolution,
        &fps,
        &full_path,
    ) {
        Ok(child) => {
            *recording_handle_clone.borrow_mut() = Some(child);
            start_button_clone.set_sensitive(false);
            stop_button_clone.set_sensitive(true);
        }
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
