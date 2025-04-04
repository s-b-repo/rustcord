use std::process::Command;

/// Lists available Wayland outputs using the `wayland-info` tool.
pub fn list_wayland_outputs() -> Vec<String> {
    let output = Command::new("wayland-info")
        .output()
        .expect("Failed to run wayland-info");

    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .lines()
        .filter(|line| line.contains("output-name"))
        .map(|line| line.to_string())
        .collect()
}
