// gui.rs
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CheckButton, ComboBoxText, Entry,
    Label, Orientation, Scale, Orientation::Horizontal, Picture
};
use glib::clone;
use std::cell::RefCell;
use std::rc::Rc;
use crate::devices::{list_camera_devices, list_microphone_sources, list_screens};
use crate::recording::{start_recording, stop_recording, PreviewPipelines};
use gstreamer as gst;
use gst::prelude::*;
use ashpd::desktop::screencast::Screencast;

pub fn build_ui(app: &Application) {
    // Create main window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Rust Wayland Recorder")
        .default_width(800)
        .default_height(700)
        .build();

    // Apply dark Discord-inspired theme via CSS
    let css = r#"
        window { background-color: #2f3136; color: #DCDDDE; }
        label { font-family: sans-serif; font-size: 14px; color: #DCDDDE; }
        button { background-color: #7289DA; color: #FFFFFF; border-radius: 4px; padding: 6px 10px; }
        button:hover { background-color: #677BC4; }
        checkbutton, comboboxtext, entry { margin: 5px; }
        scale { margin-start: 10px; margin-end: 10px; }
    "#;
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(css).expect("Failed to load CSS data");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

// Vertical container for all UI elements
let vbox = GtkBox::new(Orientation::Vertical, 8);
vbox.set_margin_start(10);
vbox.set_margin_end(10);
vbox.set_margin_top(10);
vbox.set_margin_bottom(10);


    // --- Recording Sources Section ---
    let screen_checkbox = CheckButton::with_label("Record Screen");
    screen_checkbox.set_active(true);
    let screen_combo = ComboBoxText::new();
    for screen in list_screens() {
        screen_combo.append_text(&screen);
    }
    screen_combo.set_active(Some(0));
    // Note: list_screens provides at least one entry (Wayland display). Portal will ask actual selection.

    let camera_checkbox = CheckButton::with_label("Record Camera");
    camera_checkbox.set_active(false);
    let camera_combo = ComboBoxText::new();
    for dev in list_camera_devices() {
        camera_combo.append_text(&dev);
    }
    camera_combo.set_active(Some(0));

    // --- Audio Sources Section ---
    let desktop_audio_checkbox = CheckButton::with_label("Desktop Audio");
    desktop_audio_checkbox.set_active(false);
    let desktop_vol_scale = Scale::with_range(Horizontal, 0.0, 100.0, 1.0);
    desktop_vol_scale.set_value(100.0);
    desktop_vol_scale.set_digits(0);
    desktop_vol_scale.set_hexpand(true);
    desktop_vol_scale.set_sensitive(false); // disabled until Desktop Audio is checked

    let mic_checkbox = CheckButton::with_label("Microphone");
    mic_checkbox.set_active(false);
    let mic_combo = ComboBoxText::new();
    for src in list_microphone_sources() {
        mic_combo.append_text(&src);
    }
    mic_combo.append_text("default");
    // If available, select the first mic source; otherwise "default".
    mic_combo.set_active(Some(0));
    mic_combo.set_sensitive(false);
    let mic_vol_scale = Scale::with_range(Horizontal, 0.0, 100.0, 1.0);
    mic_vol_scale.set_value(100.0);
    mic_vol_scale.set_digits(0);
    mic_vol_scale.set_hexpand(true);
    mic_vol_scale.set_sensitive(false);

    // Arrange audio options in horizontal sub-containers for better layout
    let desktop_audio_box = GtkBox::new(Orientation::Horizontal, 5);
    desktop_audio_box.append(&desktop_audio_checkbox);
    desktop_audio_box.append(&Label::new(Some("Volume:")));
    desktop_audio_box.append(&desktop_vol_scale);
    let mic_audio_box = GtkBox::new(Orientation::Horizontal, 5);
    mic_audio_box.append(&mic_checkbox);
    mic_audio_box.append(&mic_combo);
    mic_audio_box.append(&Label::new(Some("Volume:")));
    mic_audio_box.append(&mic_vol_scale);

    // --- Output Settings Section ---
    let output_folder_label = Label::new(Some("Output Folder:"));
    let chosen_folder_label = Label::new(Some("Not selected"));
    let select_folder_button = Button::with_label("Select Folder");
    select_folder_button.connect_clicked(clone!(@weak window, @weak chosen_folder_label => move |_| {
        let dialog = gtk4::FileChooserNative::new(
            Some("Select Output Folder"),
            Some(&window),
            gtk4::FileChooserAction::SelectFolder,
            Some("Select"),
            Some("Cancel")
        );
        dialog.connect_response(clone!(@weak chosen_folder_label => move |d, resp| {
            if resp == gtk4::ResponseType::Accept {
                if let Some(folder) = d.file() {
                    if let Some(path) = folder.path() {
                        chosen_folder_label.set_text(path.to_str().unwrap_or("Invalid Path"));
                    }
                }
            }
            d.destroy();
        }));
        dialog.show();
    }));

    let file_name_label = Label::new(Some("Output File Name:"));
    let file_name_entry = Entry::new();
    file_name_entry.set_placeholder_text(Some("recording_output"));
    file_name_entry.set_text("recording");  // default base name

    let format_label = Label::new(Some("Format:"));
    let format_combo = ComboBoxText::new();
    format_combo.append_text("MP4");
    format_combo.append_text("MKV");
    format_combo.append_text("WebM");
    format_combo.set_active(Some(0));

    let encoder_label = Label::new(Some("Video Encoder:"));
    let encoder_combo = ComboBoxText::new();
    // Populate based on initial format (MP4: H.264 options, MKV: H.264 + VP8/VP9, WebM: VP8/VP9)
    encoder_combo.append_text("x264");
    encoder_combo.append_text("NVENC");
    encoder_combo.append_text("VAAPI");
    encoder_combo.set_active(Some(0));
    // We'll update encoder options when format changes, see below.

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
    let preview_area = GtkBox::new(Orientation::Vertical, 0);
    preview_area.set_vexpand(true);
    preview_area.set_size_request(600, 350);

    // Main screen preview widget (Picture for screen content)
    let screen_preview = Picture::new();
    screen_preview.set_hexpand(true);
    screen_preview.set_vexpand(true);
    preview_area.append(&screen_preview);

    // Camera overlay preview widget (Picture for camera content), draggable
    let camera_preview_label = Label::new(Some("Camera Preview"));  // placeholder label
    camera_preview_label.set_size_request(160, 120);
    camera_preview_label.add_css_class("camera_preview_label");
    // We wrap the camera preview in an event box (GestureDrag can be added to a widget directly in GTK4)
    let camera_overlay = Picture::new();
    camera_overlay.set_size_request(160, 120);
    camera_overlay.hide();  // start hidden until camera is active
    // We will stack label and picture in the same place; use fixed positioning via overlay container or fixed coordinates
    let overlay_container = gtk4::Fixed::new();
    overlay_container.set_hexpand(true);
    overlay_container.set_vexpand(true);
    overlay_container.put(&camera_preview_label, 10.0, 10.0);
    overlay_container.put(&camera_overlay, 10.0, 10.0);
    preview_area.overlay(overlay_container);  // place overlay container on top of screen preview

    // Draggable functionality for camera overlay
    let initial_offset = Rc::new(RefCell::new((10.0, 10.0)));
    let drag_controller_label = gtk4::GestureDrag::new();
    camera_preview_label.add_controller(drag_controller_label);
    let drag_controller_overlay = gtk4::GestureDrag::new();
    camera_overlay.add_controller(drag_controller_overlay);
    for drag in [drag_controller_label, drag_controller_overlay] {
        // On drag update: move both label and overlay widget together
        drag.connect_drag_update(clone!(@weak overlay_container, @weak camera_preview_label, @weak camera_overlay, @weak initial_offset => move |_, offset_x, offset_y| {
            let (orig_x, orig_y) = *initial_offset.borrow();
            let new_x = orig_x + offset_x;
            let new_y = orig_y + offset_y;
    overlay_container.move_(&camera_preview_label, new_x, new_y);
    overlay_container.move_(&camera_overlay, new_x, new_y);
        }));
        // On drag end: update the stored offset
        drag.connect_drag_end(clone!(@weak initial_offset => move |_, offset_x, offset_y| {
            let (orig_x, orig_y) = *initial_offset.borrow();
            *initial_offset.borrow_mut() = (orig_x + offset_x, orig_y + offset_y);
        }));
    }

    // --- Control Buttons ---
    let start_button = Button::with_label("Start Recording");
    let stop_button = Button::with_label("Stop Recording");
    stop_button.set_sensitive(false);

    // Add all widgets to main vbox
    vbox.append(&screen_checkbox);
    vbox.append(&screen_combo);
    vbox.append(&camera_checkbox);
    vbox.append(&camera_combo);
    vbox.append(&desktop_audio_box);
    vbox.append(&mic_audio_box);
    vbox.append(&output_folder_label);
    vbox.append(&select_folder_button);
    vbox.append(&chosen_folder_label);
    vbox.append(&file_name_label);
    vbox.append(&file_name_entry);
    vbox.append(&format_label);
    vbox.append(&format_combo);
    vbox.append(&encoder_label);
    vbox.append(&encoder_combo);
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

    // Shared state for recording pipeline and preview pipelines
    let recording_pipeline: Rc<RefCell<Option<gst::Pipeline>>> = Rc::new(RefCell::new(None));
    let preview_pipelines = Rc::new(RefCell::new(PreviewPipelines { screen: None, camera: None }));

    // --- UI Interactions ---

    // Enable/disable audio controls based on checkboxes
    desktop_audio_checkbox.connect_toggled(clone!(@weak desktop_vol_scale => move |btn| {
        desktop_vol_scale.set_sensitive(btn.is_active());
    }));
    mic_checkbox.connect_toggled(clone!(@weak mic_combo, @weak mic_vol_scale => move |btn| {
        let active = btn.is_active();
        mic_combo.set_sensitive(active);
        mic_vol_scale.set_sensitive(active);
    }));

    // When format changes, update encoder options
    format_combo.connect_changed(clone!(@weak encoder_combo => move |combo| {
        if let Some(fmt) = combo.active_text() {
            encoder_combo.remove_all();
            match fmt.as_str() {
                "MP4" => {
                    encoder_combo.append_text("x264");
                    encoder_combo.append_text("NVENC");
                    encoder_combo.append_text("VAAPI");
                    encoder_combo.set_active(Some(0));
                },
                "MKV" => {
                    encoder_combo.append_text("x264");
                    encoder_combo.append_text("NVENC");
                    encoder_combo.append_text("VAAPI");
                    encoder_combo.append_text("VP8");
                    encoder_combo.append_text("VP9");
                    encoder_combo.set_active(Some(0));
                },
                "WebM" => {
                    encoder_combo.append_text("VP8");
                    encoder_combo.append_text("VP9");
                    encoder_combo.set_active(Some(0));
                },
                _ => {}
            }
        }
    }));

    // Handle toggling camera preview live feed
    camera_checkbox.connect_toggled(clone!(@weak camera_overlay, @weak camera_preview_label, @weak camera_combo, @weak preview_pipelines => move |btn| {
        if btn.is_active() {
            camera_preview_label.hide();
            camera_overlay.show();
            // Start camera preview pipeline
            if let Some(dev) = camera_combo.active_text() {
                let pipeline_desc = format!(
                    "v4l2src device={} ! videoconvert ! videoscale ! video/x-raw,width=160,height=120 ! gtk4paintablesink name=camsink",
                    dev
                );
                if let Ok(pipe) = gst::parse_launch(&pipeline_desc).and_then(|bin| bin.downcast::<gst::Pipeline>()) {
                    if let Some(sink) = pipe.by_name("camsink") {
                        if let Ok(paintable) = sink.property::<gtk4::gdk::Paintable>("paintable") {
                            camera_overlay.set_paintable(Some(&paintable));
                        }
                    }
                    pipe.set_state(gst::State::Playing).ok();
                    preview_pipelines.borrow_mut().camera = Some(pipe);
                }
            }
        } else {
            // Stop camera preview pipeline
            if let Some(pipe) = preview_pipelines.borrow_mut().camera.take() {
                let _ = pipe.set_state(gst::State::Null);
            }
            camera_overlay.hide();
            camera_preview_label.show();
        }
    }));

    // Handle toggling screen preview live feed
    screen_checkbox.connect_toggled(clone!(@weak screen_preview, @weak preview_pipelines => move |btn| {
        if btn.is_active() {
            // Start screen preview (this will prompt via portal)
            let proxy = ashpd::desktop::screencast::ScreenCast::new();
            if let Ok(proxy) = futures::executor::block_on(proxy) {
                if let Ok(session) = futures::executor::block_on(proxy.create_session()) {
                    let _ = futures::executor::block_on(proxy.select_sources(&session, ashpd::desktop::screencast::CursorMode::Embedded, ashpd::desktop::screencast::SourceType::Monitor, false));
                    if let Ok(resp) = futures::executor::block_on(proxy.start(&session, None)) {
                        if let Some(stream) = resp.streams().and_then(|streams| streams.get(0)) {
                            let node_id = stream.pipewire_node_id();
                            let pipeline_desc = format!(
                                "pipewiresrc path={} ! videoconvert ! videoscale ! video/x-raw,width=600,height=350 ! gtk4paintablesink name=screensink",
                                node_id
                            );
                            if let Ok(pipe) = gst::parse_launch(&pipeline_desc).and_then(|bin| bin.downcast::<gst::Pipeline>()) {
                                if let Some(sink) = pipe.by_name("screensink") {
                                    if let Ok(paintable) = sink.property::<gtk4::gdk::Paintable>("paintable") {
                                        screen_preview.set_paintable(Some(&paintable));
                                    }
                                }
                                pipe.set_state(gst::State::Playing).ok();
                                preview_pipelines.borrow_mut().screen = Some(pipe);
                            }
                        }
                    }
                }
            }
        } else {
            // Stop screen preview
            if let Some(pipe) = preview_pipelines.borrow_mut().screen.take() {
                let _ = pipe.set_state(gst::State::Null);
            }
            // Clear preview (set to blank)
            screen_preview.set_paintable(None);
        }
    }));

    // Start recording button
    start_button.connect_clicked(clone!(@weak screen_checkbox, @weak camera_checkbox,
                                       @weak desktop_audio_checkbox, @weak mic_checkbox,
                                       @weak screen_combo, @weak camera_combo, @weak mic_combo,
                                       @weak resolution_combo, @weak fps_combo,
                                       @weak format_combo, @weak encoder_combo,
                                       @weak file_name_entry, @weak chosen_folder_label,
                                       @weak screen_preview, @weak camera_overlay,
                                       @weak initial_offset, @weak recording_pipeline, @weak preview_pipelines,
                                       @weak start_button, @weak stop_button => move |_| {
        // Gather settings
        let record_screen = screen_checkbox.is_active();
        let record_camera = camera_checkbox.is_active();
        let record_desktop_audio = desktop_audio_checkbox.is_active();
        let record_microphone = mic_checkbox.is_active();
        let screen_selection = screen_combo.active_text().unwrap_or_else(|| "wayland-0".into());
        let camera_dev = camera_combo.active_text().unwrap_or_else(|| "/dev/video0".into());
        let mic_source = mic_combo.active_text().unwrap_or_default();
        let resolution = resolution_combo.active_text().unwrap_or_else(|| "1920x1080".into());
        let fps = fps_combo.active_text().unwrap_or_else(|| "30".into());
        let format = format_combo.active_text().unwrap_or_else(|| "MP4".into());
        let encoder = encoder_combo.active_text().unwrap_or_else(|| "x264".into());
        // Construct output file path
        let folder = chosen_folder_label.text();
        let base_name = file_name_entry.text();
        let mut full_path = if folder != "Not selected" && !folder.is_empty() {
            format!("{}/{}", folder, base_name)
        } else {
            base_name.to_string()
        };
        // Compute camera overlay offset in preview coords
        let cam_offset = *initial_offset.borrow();
        // Start recording
        match start_recording(
            record_screen,
            record_camera,
            record_desktop_audio,
            record_microphone,
            &screen_selection,
            &camera_dev,
            &mic_source,
            &format,
            &encoder,
            &resolution,
            &fps,
            &full_path,
            &screen_preview,
            &camera_overlay,
            cam_offset,
            preview_pipelines.clone()
        ) {
            Ok(pipe) => {
                *recording_pipeline.borrow_mut() = Some(pipe);
                start_button.set_sensitive(false);
                stop_button.set_sensitive(true);
            },
            Err(err) => {
                eprintln!("Error starting recording: {}", err);
            }
        }
    }));

    // Stop recording button
    stop_button.connect_clicked(clone!(@weak recording_pipeline, @weak preview_pipelines,
                                       @weak screen_checkbox, @weak camera_checkbox,
                                       @weak screen_preview, @weak camera_overlay,
                                       @weak camera_preview_label, @weak start_button, @weak stop_button => move |_| {
        if let Some(pipeline) = recording_pipeline.borrow_mut().take() {
            if let Err(e) = stop_recording(&pipeline) {
                eprintln!("Error stopping recording: {}", e);
            }
        }
        stop_button.set_sensitive(false);
        start_button.set_sensitive(true);
        // Resume previews if their checkboxes are still active
        if screen_checkbox.is_active() {
            screen_checkbox.set_active(false); // trigger toggle off then on to restart
            screen_checkbox.set_active(true);
        }
        if camera_checkbox.is_active() {
            camera_checkbox.set_active(false);
            camera_checkbox.set_active(true);
        }
    }));
}
