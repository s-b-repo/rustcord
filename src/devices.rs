// devices.rs
use std::fs;
use std::process::Command;

/// List available camera devices by scanning /dev for entries starting with "video".
pub fn list_camera_devices() -> Vec<String> {
    let mut devices = Vec::new();
    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("video") {
                    devices.push(format!("/dev/{}", name));
                }
            }
        }
    }
    devices.sort();
    devices
}

/// List available microphone input sources via PulseAudio/PipeWire.
/// This calls `pactl list short sources` and filters out monitor sources.
pub fn list_microphone_sources() -> Vec<String> {
    let pactl_out = Command::new("pactl")
        .args(&["list", "short", "sources"])
        .output()
        .unwrap_or_else(|_| Command::new("sh").arg("-c").arg("true").output().unwrap());
    // If pactl fails, return empty vec (will fall back to "default").
    let output = String::from_utf8_lossy(&pactl_out.stdout);
    let mut sources = Vec::new();
    for line in output.lines() {
        // Format: index name driver ...
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() > 1 {
            let name = cols[1];
            // Exclude monitor sources (which are desktop audio outputs)
            if !name.ends_with(".monitor") {
                sources.push(name.to_string());
            }
        }
    }
    sources.sort();
    sources
}

/// Get the PulseAudio monitor source name for the default output (desktop audio).
/// Returns something like "alsa_output.pci-XXXX.analog-stereo.monitor" or "auto_null.monitor" as default.
pub fn default_output_monitor() -> String {
    let pactl_out = Command::new("pactl")
        .arg("info")
        .output()
        .unwrap_or_else(|_| Command::new("sh").arg("-c").arg("true").output().unwrap());
    let info = String::from_utf8_lossy(&pactl_out.stdout);
    for line in info.lines() {
        if line.starts_with("Default Sink:") {
            let sink_name = line["Default Sink:".len()..].trim();
            if !sink_name.is_empty() {
                return format!("{}.monitor", sink_name);
            }
        }
    }
    // Fallback to a generic monitor name if default not found.
    "auto_null.monitor".to_string()
}

/// List available screens (monitors) for recording.
/// In Wayland we rely on the portal to select monitor, so just return the Wayland display name.
pub fn list_screens() -> Vec<String> {
    // Use WAYLAND_DISPLAY env var if available, otherwise default to ":0" for X11 (not typically used here).
    vec![std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into())]
}
