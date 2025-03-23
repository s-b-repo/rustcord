use std::process::{Child, Command, Stdio};

/// Start recording based on the provided options. If both screen and camera are selected,
/// the screen recording is managed in the primary process while the camera recording is spawned separately.
/// The function now takes additional parameters for resolution, fps, and output file.
/// For simplicity, it returns the child process for the screen recording only.
pub fn start_recording(
    record_screen: bool,
    record_camera: bool,
    screen: &str,
    camera: &str,
    resolution: &str,
    fps: &str,
    output_file: &str,
) -> Result<Child, std::io::Error> {
    if record_screen && record_camera {
        // Start screen recording with ffmpeg using chosen resolution, fps, and output file.
        let screen_child = Command::new("ffmpeg")
        .arg("-y")
        .args(&["-video_size", resolution, "-framerate", fps])
        // For Wayland, the input is taken from the provided screen (typically the WAYLAND_DISPLAY).
        .args(&["-f", "wayland", "-i", screen])
        .arg(output_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

        // Create a camera output file name by appending "_camera" before the extension.
        let camera_output = if let Some(dot_index) = output_file.rfind('.') {
            let (base, ext) = output_file.split_at(dot_index);
            format!("{}_camera{}", base, ext)
        } else {
            format!("{}_camera", output_file)
        };

        // Spawn a separate process for camera recording.
        let _camera_child = Command::new("ffmpeg")
        .arg("-y")
        .args(&["-f", "v4l2", "-i", camera])
        .arg(camera_output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
        // Return the screen recording process handle.
        Ok(screen_child)
    } else if record_screen {
        // Only screen recording.
        Command::new("ffmpeg")
        .arg("-y")
        .args(&["-video_size", resolution, "-framerate", fps])
        .args(&["-f", "wayland", "-i", screen])
        .arg(output_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    } else if record_camera {
        // Only camera recording. Resolution and FPS are irrelevant here.
        Command::new("ffmpeg")
        .arg("-y")
        .args(&["-f", "v4l2", "-i", camera])
        .arg(output_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No recording option selected",
        ))
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
