use std::process::{Child, Command, Stdio};

/// Start recording based on the provided options. If both screen and camera are selected,
/// the screen recording is managed in the primary process while the camera recording is spawned separately.
/// For simplicity, this function returns the child process for the screen recording only.
/// In production, you’d want to track and manage both processes.
pub fn start_recording(
    record_screen: bool,
    record_camera: bool,
    screen: &str,
    camera: &str,
) -> Result<Child, std::io::Error> {
    if record_screen && record_camera {
        // Start screen recording with ffmpeg.
        let screen_child = Command::new("ffmpeg")
        .arg("-y")
        .args(&["-video_size", "1920x1080", "-framerate", "30"])
        // For Wayland, the input is taken from the provided screen (typically the WAYLAND_DISPLAY).
        .args(&["-f", "wayland", "-i", screen])
        .arg("screen_recording.mp4")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

        // Spawn a separate process for camera recording.
        let _camera_child = Command::new("ffmpeg")
        .arg("-y")
        .args(&["-f", "v4l2", "-i", camera])
        .arg("camera_recording.mp4")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
        // Return the screen recording process handle.
        Ok(screen_child)
    } else if record_screen {
        // Only screen recording.
        Command::new("ffmpeg")
        .arg("-y")
        .args(&["-video_size", "1920x1080", "-framerate", "30"])
        .args(&["-f", "wayland", "-i", screen])
        .arg("screen_recording.mp4")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    } else if record_camera {
        // Only camera recording.
        Command::new("ffmpeg")
        .arg("-y")
        .args(&["-f", "v4l2", "-i", camera])
        .arg("camera_recording.mp4")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "No recording option selected"))
    }
}

/// Stop the recording process.
pub fn stop_recording(handle: &mut Option<Child>) -> Result<(), std::io::Error> {
    if let Some(child) = handle {
        child.kill()?;
        *handle = None;
    }
    Ok(())
}
