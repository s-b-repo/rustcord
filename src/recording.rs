// recording.rs
use std::cell::RefCell;
use std::rc::Rc;
use gstreamer as gst;
use gst::prelude::*;
use ashpd::desktop::screencast::{CursorMode, SourceType};
use ashpd::desktop::Request;
use ashpd::desktop::screencast::Screencast;
use gtk4::prelude::WidgetExt;
use crate::devices;
use ashpd::WindowIdentifier;


/// A convenience struct to hold active preview pipelines (to be stopped when recording starts).
pub struct PreviewPipelines {
    pub screen: Option<gst::Pipeline>,
    pub camera: Option<gst::Pipeline>,
}

/// Start the recording with the given options. Returns the GStreamer pipeline on success.
pub fn start_recording(
    record_screen: bool,
    record_camera: bool,
    record_desktop_audio: bool,
    record_microphone: bool,
    screen_name: &str,           // e.g., "wayland-0" (unused directly, portal handles selection)
    camera_dev: &str,           // e.g., "/dev/video0"
    mic_source: &str,           // PulseAudio source name for mic (or "default")
    output_format: &str,        // "MP4", "MKV", or "WebM"
    video_encoder: &str,        // "x264", "NVENC", "VAAPI", "VP8", "VP9"
    resolution: &str,           // e.g., "1920x1080"
    fps: &str,                  // e.g., "30"
    output_path: &str,
    preview_widget: &gtk4::Picture,  // GTK Picture widget for screen preview
    preview_overlay: &gtk4::Picture, // GTK Picture widget for camera overlay preview
    cam_overlay_offset: (f64, f64),   // Camera overlay position in preview area (x,y)
    preview_state: Rc<RefCell<PreviewPipelines>>, // to stop preview pipelines if running
) -> Result<gst::Pipeline, String> {
    // Make sure at least one of screen, camera, or audio is selected
    if !record_screen && !record_camera && !record_desktop_audio && !record_microphone {
        return Err("No recording source selected".into());
    }

    // Stop any running preview pipelines to free devices (avoids conflicts)
    {
        let mut previews = preview_state.borrow_mut();
        if let Some(screen_pipe) = previews.screen.take() {
            let _ = screen_pipe.set_state(gst::State::Null);
        }
        if let Some(cam_pipe) = previews.camera.take() {
            let _ = cam_pipe.set_state(gst::State::Null);
        }
    }

    // Parse resolution and FPS
    let (out_width, out_height) = if let Some((w, h)) = resolution.split_once('x') {
        (w.parse::<i32>().unwrap_or(1920), h.parse::<i32>().unwrap_or(1080))
    } else {
        (1920, 1080)
    };
    let fps_val: i32 = fps.parse().unwrap_or(30);

    // Determine output container muxer and file extension
    let (mux_element, file_ext) = match output_format {
        "MP4" => ("mp4mux", "mp4"),
        "MKV" => ("matroskamux", "mkv"),
        "WebM" => ("webmmux", "webm"),
        _ => ("matroskamux", "mkv"),
    };
    // Ensure the output_path has the correct extension
    let mut output_path_fixed = output_path.to_string();
    if !output_path_fixed.to_lowercase().ends_with(&format!(".{}", file_ext).to_lowercase()) {
        output_path_fixed.push_str(&format!(".{}", file_ext));
    }

    // Determine video encoder element based on selection
    let video_enc_element = match video_encoder {
        "x264" => "x264enc",                     // CPU x264 encoding
        "NVENC" => "nvh264enc",                 // NVIDIA NVENC (requires plugin and driver)
        "VAAPI" => "vaapih264enc",              // VAAPI H.264 encoding (requires plugin and driver)
        "VP8" => "vp8enc",                      // VP8 encoding (for WebM)
        "VP9" => "vp9enc",                      // VP9 encoding (for WebM)
        _ => "x264enc",
    };
    // If format is WebM but encoder is not VP8/VP9, override to VP8 (WebM needs VP8/VP9 + Vorbis/Opus)
    let mut video_enc_element_final = video_enc_element;
    if output_format == "WebM" && !(video_encoder == "VP8" || video_encoder == "VP9") {
        video_enc_element_final = "vp8enc";
    }
    // If format is MP4 and a non-H.264 encoder was chosen (VP8/VP9), we override to x264 (MP4 expects H.264/AAC).
    if output_format == "MP4" && (video_enc_element_final.starts_with("vp8") || video_enc_element_final.starts_with("vp9")) {
        video_enc_element_final = "x264enc";
    }

    // Determine audio encoder based on format
    // MP4/MKV: use AAC via voaacenc (requires GStreamer ugly) or avenc_aac as fallback.
    // WebM: use Vorbis encoder.
    let audio_enc_element = if output_format == "WebM" {
        "vorbisenc"
    } else {
        // AAC encoder: use voaacenc if available
        "voaacenc"
    };

    // Prepare PulseAudio source names for audio
    let monitor_src = devices::default_output_monitor();
    let desktop_src = if record_desktop_audio {
        monitor_src.as_str()
    } else {
        " "  // not used
    };
    let mic_src = if record_microphone {
        if mic_source.is_empty() { "default" } else { mic_source }
    } else {
        " "
    };

    // If screen recording is requested, obtain PipeWire stream node ID via XDG portal.
    let mut pipewire_node_id: Option<u32> = None;
    if record_screen {
        // Use ashpd (xdg-desktop-portal) to select screen and get a PipeWire stream.
        let proxy = Screencast::new().await.map_err(|e| format!("Portal error: {}", e))?;
        // Open a portal session for screen capture (monitor only, allow user to choose).
        let session = proxy.create_session().await.map_err(|e| format!("Portal session error: {}", e))?;
        proxy.select_sources(&session, CursorMode::Embedded, SourceType::Monitor, false)
     .await.map_err(|e| format!("Portal source selection failed: {}", e))?;
        let start_resp = proxy.start(&session, &WindowIdentifier::default())
     .await.map_err(|e| format!("Portal start failed: {}", e))?;
        let streams = start_resp.streams().ok_or("No stream returned by portal")?;
        if let Some(stream) = streams.get(0) {
            pipewire_node_id = Some(stream.pipewire_node_id());
        } else {
            return Err("No screen stream selected".into());
        }
    }

    // Build the GStreamer pipeline description string
    let mut pipeline_desc = format!("{} name=mux ! filesink location=\"{}\" sync=true ", mux_element, output_path_fixed);
    // Video branch
    if record_screen || record_camera {
        // Tee for splitting to file and preview
        pipeline_desc.push_str("tee name=t ! queue ! ");
        pipeline_desc.push_str(&format!("{} ! mux. ", video_enc_element_final));
    }
    // If screen recording, add screen source -> compositor or tee
    if record_screen && record_camera {
        // Both screen and camera: use compositor to overlay camera on screen
        // Compute camera overlay position and size in output resolution
        let (cam_x, cam_y) = cam_overlay_offset;
        let cam_x_out = ((cam_x / 600.0) * out_width as f64).round() as i32;
        let cam_y_out = ((cam_y / 350.0) * out_height as f64).round() as i32;
        let cam_w_out = ((160.0 / 600.0) * out_width as f64).round() as i32;
        let cam_h_out = ((120.0 / 350.0) * out_height as f64).round() as i32;
        // Screen source branch
        pipeline_desc.push_str(&format!(
            "pipewiresrc {} ! videoconvert ! videoscale ! video/x-raw,framerate={}/1 ! comp.sink_0 ",
            pipewire_node_id.map(|id| format!("path={}", id)).unwrap_or_default(),
            fps_val
        ));
        // Camera source branch
        pipeline_desc.push_str(&format!(
            "v4l2src device={} ! videoconvert ! videoscale ! video/x-raw,width={},height={},framerate={}/1 ! comp.sink_1 ",
            camera_dev, cam_w_out, cam_h_out, fps_val
        ));
        // Compositor element
        pipeline_desc.push_str(&format!(
            "compositor name=comp sink_1::xpos={} sink_1::ypos={} ! videoconvert ! videoscale ! video/x-raw,width={},height={},framerate={}/1 ! t. ",
            cam_x_out, cam_y_out, out_width, out_height, fps_val
        ));
    } else if record_screen && !record_camera {
        // Only screen video
        pipeline_desc.push_str(&format!(
            "pipewiresrc {} ! videoconvert ! videoscale ! video/x-raw,width={},height={},framerate={}/1 ! t. ",
            pipewire_node_id.map(|id| format!("path={}", id)).unwrap_or_default(),
            out_width, out_height, fps_val
        ));
    } else if record_camera && !record_screen {
        // Only camera video
        pipeline_desc.push_str(&format!(
            "v4l2src device={} ! videoconvert ! videoscale ! video/x-raw,width={},height={},framerate={}/1 ! t. ",
            camera_dev, out_width, out_height, fps_val
        ));
    }
    // Preview branch from tee (video): render to GTK4 widget
    if record_screen || record_camera {
        pipeline_desc.push_str("t. ! queue ! gtk4paintablesink name=videosink ");
    }
    // Audio branch(es)
    if record_desktop_audio && record_microphone {
        // Both desktop and mic: mix them
        pipeline_desc.push_str(&format!(
            "pulsesrc device={} ! volume volume=1.0 ! audioconvert ! audioresample ! mix. \
             pulsesrc device={} ! volume volume=1.0 ! audioconvert ! audioresample ! mix. \
             audiomixer name=mix ! {} ! mux. ",
            desktop_src, mic_src, audio_enc_element
        ));
    } else if record_desktop_audio {
        pipeline_desc.push_str(&format!(
            "pulsesrc device={} ! volume volume=1.0 ! audioconvert ! audioresample ! {} ! mux. ",
            desktop_src, audio_enc_element
        ));
    } else if record_microphone {
        pipeline_desc.push_str(&format!(
            "pulsesrc device={} ! volume volume=1.0 ! audioconvert ! audioresample ! {} ! mux. ",
            mic_src, audio_enc_element
        ));
    }
    // Create the pipeline from the description
    gst::debug_bin_to_dot_file_with_ts = || {}; // (Optional: for debug)
    let pipeline = gst::parse_launch(&pipeline_desc)
        .map_err(|e| format!("Failed to create pipeline: {}", e))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| "Failed to downcast to Pipeline".to_string())?;

    // Start the pipeline
    pipeline.set_state(gst::State::Playing).map_err(|e| format!("Failed to start pipeline: {}", e))?;

    // Attach the pipeline's video sink paintable to the UI preview widgets
    if let Some(video_sink) = pipeline.by_name("videosink") {
        if let Ok(paintable) = video_sink.property::<gtk4::gdk::Paintable>("paintable") {
            if record_screen {
                // If screen is being captured, show preview on the large preview widget
                preview_widget.set_paintable(Some(&paintable));
            } else if record_camera {
                // If only camera, show it in the main preview area as well (since no screen background)
                preview_widget.set_paintable(Some(&paintable));
            }
            // Also, if camera is being recorded and overlay is used, set the camera overlay widget paintable
            // (In our pipeline, the camera is composited into the same videosink if screen+camera,
            // so the single paintable covers both. If only camera, we already set preview_widget.)
            if record_camera && record_screen {
                // The composited output already contains camera, so preview_widget is enough.
                // We hide the separate camera overlay widget during recording to avoid duplication.
                preview_overlay.hide();
            }
        }
    }
    Ok(pipeline)
}

/// Stop the recording by stopping the GStreamer pipeline.
pub fn stop_recording(pipeline: &gst::Pipeline) -> Result<(), String> {
    pipeline.set_state(gst::State::Null).map_err(|e| format!("Failed to stop pipeline: {}", e))?;
    Ok(())
}
