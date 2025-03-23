use std::fs;

/// List available camera devices by scanning /dev for entries starting with "video".
pub fn list_camera_devices() -> Vec<String> {
    let mut devices = Vec::new();
    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name() {
                if file_name.to_string_lossy().starts_with("video") {
                    devices.push(path.to_string_lossy().into_owned());
                }
            }
        }
    }
    devices
}

/// List screens available for recording.
/// In this simplified example, we assume the default Wayland display is the only option.
/// For a more complete implementation, you might query the compositor for connected outputs.
pub fn list_screens() -> Vec<String> {
    vec![std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| ":0".into())]
}
