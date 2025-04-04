use gtk4::{
    Box as GtkBox, Button, CheckButton, ComboBoxText, Entry, Label, Orientation,
    ProgressBar, Separator, SpinButton, Adjustment,
};
use gtk4::glib::{clone, timeout_add_seconds_local, Continue};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::encoder::{
    start_recording_with_audio_sources,
    stop_recording,
    pause_recording,
    resume_recording,
    VOLUME_DATA,
};
use crate::devices::audio::get_audio_sources;

pub fn build_controls() -> GtkBox {
    let vbox = GtkBox::new(Orientation::Vertical, 10);

    // Audio sources + VU meters
    vbox.append(&Label::new(Some("Audio Sources:")));
    let audio_sources = get_audio_sources();
    let mut meters = Vec::new();

    for (_id, name, desc) in audio_sources {
        let row = GtkBox::new(Orientation::Horizontal, 5);
        let check = CheckButton::with_label(&format!("{} ({})", name, desc));
        let meter = ProgressBar::new();
        meter.set_fraction(0.0);

        row.append(&check);
        row.append(&meter);
        vbox.append(&row);

        // In real usage, you'd track which checkboxes are active. We'll keep it simple.
        meters.push((name.clone(), meter));
    }

    vbox.append(&Separator::new(Orientation::Horizontal));

    // Output Settings
    vbox.append(&Label::new(Some("Output Settings:")));

    // Filename
    let filename_entry = Entry::new();
    filename_entry.set_placeholder_text(Some("output_filename"));
    vbox.append(&Label::new(Some("Filename:")));
    vbox.append(&filename_entry);

    // Format dropdown
    let format_box = ComboBoxText::new();
    format_box.append(Some("webm"), Some("WebM"));
    format_box.append(Some("mp4"), Some("MP4"));
    format_box.append(Some("mkv"), Some("MKV"));
    format_box.set_active(Some(0));
    vbox.append(&Label::new(Some("Format:")));
    vbox.append(&format_box);

    // Resolution, up to 8K
    let width_spin = SpinButton::new(
        Some(&Adjustment::new(1280.0, 320.0, 7680.0, 1.0, 10.0, 0.0)),
        1.0,
        0
    );
    let height_spin = SpinButton::new(
        Some(&Adjustment::new(720.0, 240.0, 4320.0, 1.0, 10.0, 0.0)),
        1.0,
        0
    );
    let fps_spin = SpinButton::new(
        Some(&Adjustment::new(30.0, 5.0, 240.0, 1.0, 10.0, 0.0)),
        1.0,
        0
    );
    let bitrate_spin = SpinButton::new(
        Some(&Adjustment::new(4096.0, 1000.0, 50000.0, 512.0, 1024.0, 0.0)),
        1.0,
        0
    );

    let res_row = GtkBox::new(Orientation::Horizontal, 5);
    res_row.append(&Label::new(Some("Width:")));
    res_row.append(&width_spin);
    res_row.append(&Label::new(Some("Height:")));
    res_row.append(&height_spin);
    vbox.append(&res_row);

    let fps_row = GtkBox::new(Orientation::Horizontal, 5);
    fps_row.append(&Label::new(Some("Framerate:")));
    fps_row.append(&fps_spin);
    fps_row.append(&Label::new(Some("Bitrate (kbps):")));
    fps_row.append(&bitrate_spin);
    vbox.append(&fps_row);

    // Buttons
    let btn_box = GtkBox::new(Orientation::Horizontal, 10);
    let start_btn = Button::with_label("Start Recording");
    let stop_btn = Button::with_label("Stop Recording");
    let pause_btn = Button::with_label("Pause");

    stop_btn.set_sensitive(false);

    let is_recording = Rc::new(RefCell::new(false));
    let is_paused = Rc::new(RefCell::new(false));

    start_btn.connect_clicked(clone!(@strong is_recording => move |_| {
        if !*is_recording.borrow() {
            let filename = filename_entry.text().to_string();
            if filename.is_empty() {
                eprintln!("Filename is required!");
                return;
            }
            let format = format_box.active_id().unwrap_or_else(|| "webm".to_string());
            let width = width_spin.value_as_int() as u32;
            let height = height_spin.value_as_int() as u32;
            let fps = fps_spin.value_as_int() as u32;
            let bitrate = bitrate_spin.value_as_int() as u32;

            // For demonstration, we won't parse audio checkboxes here
            let selected_sources = Vec::new();
            start_recording_with_audio_sources(
                selected_sources,
                format,
                filename,
                Some((width, height)),
                Some(fps),
                Some(bitrate),
            );

            *is_recording.borrow_mut() = true;
            stop_btn.set_sensitive(true);
        }
    }));

    stop_btn.connect_clicked(clone!(@strong is_recording => move |_| {
        if *is_recording.borrow() {
            stop_recording();
            *is_recording.borrow_mut() = false;
            stop_btn.set_sensitive(false);
        }
    }));

    pause_btn.connect_clicked(clone!(@strong is_paused => move |btn| {
        if *is_paused.borrow() {
            resume_recording();
            btn.set_label("Pause");
            *is_paused.borrow_mut() = false;
        } else {
            pause_recording();
            btn.set_label("Resume");
            *is_paused.borrow_mut() = true;
        }
    }));

    btn_box.append(&start_btn);
    btn_box.append(&stop_btn);
    btn_box.append(&pause_btn);
    vbox.append(&btn_box);

    // Update VU meters from VOLUME_DATA
    timeout_add_seconds_local(1, clone!(@strong meters => move || {
        let volumes = VOLUME_DATA.lock().unwrap();
        for (name, meter) in &meters {
            if let Some(vol) = volumes.get(name) {
                meter.set_fraction(*vol.min(&1.0));
            }
        }
        Continue(true)
    }));

    vbox
}
