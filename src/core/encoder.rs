use gstreamer as gst;
use gstreamer::prelude::*;
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use once_cell::sync::Lazy;
use std::time::Duration;

use crate::core::pipewire::init_pipewire;
use crate::extensions::{
    scene_switcher::{SceneSwitcher, Scene, SceneSource},
    overlays::OverlayManager,
    streaming::MultiStreamingManager,
    hardware_accel::setup_unified_hardware_accel,
    plugin_system::{PluginManager, GLOBAL_PLUGIN_MANAGER},
};

// For reading peak audio levels
pub static VOLUME_DATA: Lazy<Arc<Mutex<HashMap<String, f64>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

// Global references for advanced managers/pipeline
static mut GLOBAL_PIPELINE: Option<gst::Pipeline> = None;
static mut GLOBAL_SCENE_SWITCHER: Option<SceneSwitcher> = None;
static mut GLOBAL_OVERLAY_MANAGER: Option<OverlayManager> = None;
static mut GLOBAL_STREAMING_MANAGER: Option<MultiStreamingManager> = None;

// A separate pipeline for "recording" to file
static RECORDING_PIPELINE: Mutex<Option<gst::Pipeline>> = Mutex::new(None);

/// Builds a top-level pipeline with PipeWire (screen) + v4l2src (webcam),
/// a compositor, hardware acceleration, scene switching, overlays, streaming, plugin system
pub fn init_pipeline_with_advanced_features() -> Result<gst::Pipeline> {
    gst::init()?;
    init_pipewire();

    // Create main pipeline
    let pipeline = gst::Pipeline::new(Some("waycord_pipeline"));

    // Basic elements
    let pw_src = gst::ElementFactory::make("pipewiresrc", Some("pw_src"))?;
    let cam_src = gst::ElementFactory::make("v4l2src", Some("cam_src"))?;
    let compositor = gst::ElementFactory::make("compositor", Some("comp"))?;
    let videoconvert = gst::ElementFactory::make("videoconvert", Some("videoconvert"))?;
    let queue = gst::ElementFactory::make("queue", Some("queue"))?;

    pipeline.add_many(&[&pw_src, &cam_src, &compositor, &videoconvert, &queue])?;

    // Link pw_src -> compositor.sink_0
    let pw_pad = compositor.request_pad_simple("sink_0").ok_or_else(|| anyhow!("No sink_0 pad"))?;
    pw_pad.set_property("xpos", 0i32);
    pw_pad.set_property("ypos", 0i32);
    pw_src.link(&compositor)?;

    // Link cam_src -> compositor.sink_1
    let cam_pad = compositor.request_pad_simple("sink_1").ok_or_else(|| anyhow!("No sink_1 pad"))?;
    cam_pad.set_property("xpos", 100i32);
    cam_pad.set_property("ypos", 100i32);
    cam_src.link(&compositor)?;

    // Link compositor -> videoconvert -> queue
    gst::Element::link_many(&[&compositor, &videoconvert, &queue])?;

    // Attempt hardware acceleration
    let _ = setup_unified_hardware_accel(&pipeline, &videoconvert, &queue);

    // Scene Switcher
    let main_context = glib::MainContext::default();
    let initial_scene = Scene {
        name: "Scene0".into(),
        sources: vec![SceneSource {
            element: pw_src.clone(),
            pad_index: 0,
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
            alpha: 1.0,
        }],
    };
    let scene_switcher = SceneSwitcher::new(
        pipeline.clone(),
        compositor.clone(),
        vec![initial_scene],
        Duration::from_secs(1),
        main_context.clone(),
    );

    // Overlay Manager
    let overlay_mgr = OverlayManager::new(pipeline.clone(), main_context.clone());

    // Multi-streaming
    let stream_mgr = MultiStreamingManager::new(pipeline.clone(), 4000, 1000, 8000);

    // Plugin System: create a global plugin manager
    unsafe {
        GLOBAL_PLUGIN_MANAGER = Some(PluginManager::new());
    }

    unsafe {
        GLOBAL_PIPELINE = Some(pipeline.clone());
        GLOBAL_SCENE_SWITCHER = Some(scene_switcher);
        GLOBAL_OVERLAY_MANAGER = Some(overlay_mgr);
        GLOBAL_STREAMING_MANAGER = Some(stream_mgr);
    }

    println!("Advanced pipeline initialized.");
    Ok(pipeline)
}

// Global accessors
pub fn get_global_pipeline() -> Option<&'static gst::Pipeline> {
    unsafe { GLOBAL_PIPELINE.as_ref() }
}
pub fn get_global_scene_switcher() -> Option<&'static SceneSwitcher> {
    unsafe { GLOBAL_SCENE_SWITCHER.as_ref() }
}
pub fn get_global_overlay_manager() -> Option<&'static OverlayManager> {
    unsafe { GLOBAL_OVERLAY_MANAGER.as_ref() }
}
pub fn get_global_streaming_manager() -> Option<&'static MultiStreamingManager> {
    unsafe { GLOBAL_STREAMING_MANAGER.as_ref() }
}

// -- Recording pipeline for file output (start/stop/pause/resume) --

pub fn start_recording_with_audio_sources(
    audio_sources: Vec<String>,
    format: String,
    filename: String,
    resolution: Option<(u32, u32)>,
    framerate: Option<u32>,
    bitrate: Option<u32>,
) {
    let (width, height) = resolution.unwrap_or((1280, 720));
    let fps = framerate.unwrap_or(30);
    let br = bitrate.unwrap_or(4096);

    let (audio_enc, muxer) = match format.as_str() {
        "webm" => ("opusenc", "webmmux name=mux"),
        "mp4" => ("faac", "mp4mux name=mux"),
        "mkv" => ("vorbisenc", "matroskamux name=mux"),
        other => {
            eprintln!("Unknown format '{}', defaulting to webm", other);
            ("opusenc", "webmmux name=mux")
        }
    };

    let mut audio_parts = String::new();
    for source in &audio_sources {
        audio_parts.push_str(&format!(
            "pwaudiosrc target-object={} ! level name=level_{} interval=100000000 ! \
             audioconvert ! audioresample ! queue ! {} ! mux. ",
            source, source, audio_enc
        ));
    }

    // Additional example: a second feed from v4l2src overlay
    let pipeline_str = format!(
        concat!(
            "compositor name=comp sink_1::xpos=100 sink_1::ypos=100 ! ",
            "videoconvert ! x264enc bitrate={br} tune=zerolatency speed-preset=ultrafast ! queue ! mux. ",
            "pipewiresrc ! video/x-raw,width={width},height={height},framerate={fps}/1 ! comp.sink_0 ",
            "v4l2src device=/dev/video0 ! video/x-raw,width=320,height=240 ! comp.sink_1 ",
            "{audio_parts} ",
            "{muxer} ! filesink location=\"{filename}.{ext}\""
        ),
        br=br,
        width=width,
        height=height,
        fps=fps,
        audio_parts=audio_parts,
        muxer=muxer,
        filename=filename,
        ext=format,
    );

    gst::init().ok();
    let pipeline = gst::parse_launch(&pipeline_str)
        .expect("Failed to create recording pipeline")
        .downcast::<gst::Pipeline>()
        .unwrap();

    // Watch for audio levels
    let bus = pipeline.bus().unwrap();
    bus.add_watch(move |_, msg| {
        if let gst::MessageView::Element(elem) = msg.view() {
            if let Some(structure) = elem.structure() {
                if structure.name() == "level" {
                    if let Ok(peaks) = structure.get::<gst::List>("peak") {
                        let elem_name = elem.src().map(|s| s.path_string()).unwrap_or_default();
                        if elem_name.starts_with("level_") {
                            let source_name = elem_name.trim_start_matches("level_").to_string();
                            if let Some(first_peak) = peaks.iter().next() {
                                if let Ok(peak_val) = first_peak.get::<f64>() {
                                    VOLUME_DATA.lock().unwrap().insert(source_name, peak_val);
                                }
                            }
                        }
                    }
                }
            }
        }
        glib::Continue(true)
    }).unwrap();

    pipeline.set_state(gst::State::Playing).unwrap();
    *RECORDING_PIPELINE.lock().unwrap() = Some(pipeline);

    println!("Recording pipeline started to file: {}.{}", filename, format);
}

pub fn stop_recording() {
    let mut guard = RECORDING_PIPELINE.lock().unwrap();
    if let Some(pipe) = guard.take() {
        pipe.set_state(gst::State::Null).unwrap();
        println!("Recording pipeline stopped.");
    }
}

pub fn pause_recording() {
    let guard = RECORDING_PIPELINE.lock().unwrap();
    if let Some(pipe) = guard.as_ref() {
        pipe.set_state(gst::State::Paused).unwrap();
        println!("Recording pipeline paused.");
    }
}

pub fn resume_recording() {
    let guard = RECORDING_PIPELINE.lock().unwrap();
    if let Some(pipe) = guard.as_ref() {
        pipe.set_state(gst::State::Playing).unwrap();
        println!("Recording pipeline resumed.");
    }
}
