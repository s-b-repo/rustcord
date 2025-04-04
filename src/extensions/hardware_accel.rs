use gstreamer as gst;
use gstreamer::prelude::*;
use anyhow::{Result, anyhow};
use std::fs;

#[derive(Debug)]
pub enum AccelMode {
    VAAPI,
    NVENC,
    AMF,
    Software,
}

pub fn setup_unified_hardware_accel(
    pipeline: &gst::Pipeline,
    upstream: &gst::Element,
    downstream: &gst::Element,
) -> Result<AccelMode> {
    // If GPU usage is too high, fallback immediately
    if let Ok(gpu_usage) = read_gpu_usage() {
        if gpu_usage > 90.0 {
            println!("GPU usage {gpu_usage}% is too high, using software x264.");
            use_software_x264(pipeline, upstream, downstream)?;
            return Ok(AccelMode::Software);
        }
    }

    // Try VAAPI
    if let Ok(vaapienc) = gst::ElementFactory::make("vaapih264enc", Some("vaapi_enc")) {
        pipeline.add(&vaapienc)?;
        vaapienc.sync_state_with_parent()?;
        if upstream.link(&vaapienc).is_ok() && vaapienc.link(downstream).is_ok() {
            println!("Using VAAPI acceleration.");
            return Ok(AccelMode::VAAPI);
        } else {
            pipeline.remove(&vaapienc)?;
        }
    }

    // Try NVENC
    if let Ok(nvenc_enc) = gst::ElementFactory::make("nvh264enc", Some("nvenc_enc")) {
        pipeline.add(&nvenc_enc)?;
        nvenc_enc.sync_state_with_parent()?;
        if upstream.link(&nvenc_enc).is_ok() && nvenc_enc.link(downstream).is_ok() {
            println!("Using NVENC acceleration.");
            return Ok(AccelMode::NVENC);
        } else {
            pipeline.remove(&nvenc_enc)?;
        }
    }

    // Try AMF (AMD)
    if let Ok(amf_enc) = gst::ElementFactory::make("amfenc_h264", Some("amf_enc")) {
        pipeline.add(&amf_enc)?;
        amf_enc.sync_state_with_parent()?;
        if upstream.link(&amf_enc).is_ok() && amf_enc.link(downstream).is_ok() {
            println!("Using AMD AMF acceleration.");
            return Ok(AccelMode::AMF);
        } else {
            pipeline.remove(&amf_enc)?;
        }
    }

    // Otherwise, software fallback
    println!("No hardware encoders found or linking failed. Falling back to x264 software.");
    use_software_x264(pipeline, upstream, downstream)?;
    Ok(AccelMode::Software)
}

fn use_software_x264(pipeline: &gst::Pipeline, upstream: &gst::Element, downstream: &gst::Element) -> Result<()> {
    let x264enc = gst::ElementFactory::make("x264enc", Some("soft_x264"))
        .map_err(|_| anyhow!("x264enc plugin not available."))?;
    pipeline.add(&x264enc)?;
    x264enc.sync_state_with_parent()?;

    upstream.link(&x264enc)
        .map_err(|_| anyhow!("Failed to link upstream -> x264enc."))?;
    x264enc.link(downstream)
        .map_err(|_| anyhow!("Failed to link x264enc -> downstream."))?;

    Ok(())
}

/// Example: read GPU usage from a file or system interface
fn read_gpu_usage() -> Result<f64> {
    // For demonstration, read a mock usage from /tmp/fake_gpu_usage
    if let Ok(contents) = fs::read_to_string("/tmp/fake_gpu_usage") {
        if let Ok(val) = contents.trim().parse::<f64>() {
            return Ok(val.clamp(0.0, 100.0));
        }
    }
    // If not found, assume usage is minimal
    Ok(0.0)
}
