// src/main.rs
use anyhow::{Context, Result};
use crossbeam::channel::{bounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod gui; // Import the GUI module (in src/gui.rs)

/// Structure representing a video frame.
#[derive(Debug, Clone)]
pub struct Frame {
    pub timestamp: Instant,
    pub width: u32,
    pub height: u32,
    /// RGBA pixel data in row–major order.
    pub data: Vec<u8>,
}

/// Screen capture module using the scrap crate.
pub mod screen_capture {
    use super::*;
    use scrap::{Capturer, Display};

    pub fn run(sender: Sender<Frame>, stop_flag: Arc<AtomicBool>) -> Result<()> {
        // Select the primary display.
        let display = Display::primary().context("Failed to get primary display")?;
        let (width, height) = (display.width(), display.height());
        let mut capturer = Capturer::new(display).context("Failed to begin capture")?;
        println!("Screen capture started: {}x{}", width, height);

        while !stop_flag.load(Ordering::SeqCst) {
            let frame = match capturer.frame() {
                Ok(frame) => frame,
                Err(error) => {
                    if error.kind() == std::io::ErrorKind::WouldBlock {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    } else {
                        return Err(error.into());
                    }
                }
            };

            // Convert the raw BGRX frame to RGBA.
            let mut rgba_data = Vec::with_capacity(width * height * 4);
            for chunk in frame.chunks(4) {
                if chunk.len() >= 3 {
                    rgba_data.push(chunk[2]); // R
                    rgba_data.push(chunk[1]); // G
                    rgba_data.push(chunk[0]); // B
                    rgba_data.push(255u8);    // A
                }
            }

            let frame = Frame {
                timestamp: Instant::now(),
                width: width as u32,
                height: height as u32,
                data: rgba_data,
            };

            let _ = sender.try_send(frame);
            thread::sleep(Duration::from_millis(33)); // ~30fps
        }
        Ok(())
    }
}

/// Camera capture module using OpenCV.
pub mod camera_capture {
    use super::*;
    use opencv::prelude::*;
    use opencv::videoio::{VideoCapture, CAP_ANY};

    pub fn run(sender: Sender<Frame>, stop_flag: Arc<AtomicBool>) -> Result<()> {
        // Open default camera.
        let mut cam = VideoCapture::new(0, CAP_ANY).context("Failed to open default camera")?;
        if !VideoCapture::is_opened(&cam).context("Camera not opened")? {
            return Err(anyhow::anyhow!("Camera could not be opened"));
        }
        println!("Camera capture started.");

        // Determine camera properties.
        let cam_width = cam.get(opencv::videoio::CAP_PROP_FRAME_WIDTH)? as u32;
        let cam_height = cam.get(opencv::videoio::CAP_PROP_FRAME_HEIGHT)? as u32;

        while !stop_flag.load(Ordering::SeqCst) {
            let mut mat = opencv::core::Mat::default();
            cam.read(&mut mat).context("Failed to read from camera")?;
            if mat.empty() {
                continue;
            }
            let mat = mat.to_mat().context("Failed to convert camera frame to mat")?;
            let mat_bgr = mat.try_clone().context("Clone error")?;
            opencv::imgproc::cvt_color(&mat_bgr, &mut mat.clone(), opencv::core::COLOR_BGR2RGBA)
                .map_err(|e| anyhow::anyhow!("Error converting colors: {:?}", e))?;

            let size = (cam_width * cam_height * 4) as usize;
            let mut buf = vec![0u8; size];
            opencv::core::copy_to_slice(&mat, &mut buf).context("Failed to copy camera data")?;

            let frame = Frame {
                timestamp: Instant::now(),
                width: cam_width,
                height: cam_height,
                data: buf,
            };

            let _ = sender.try_send(frame);
            thread::sleep(Duration::from_millis(33));
        }
        Ok(())
    }
}

/// Compositor module for dynamic layering.
pub mod compositor {
    use super::*;
    use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
    use image::imageops::overlay;

    /// Overlay configuration structure.
    #[derive(Clone, Copy)]
    pub struct OverlayConfig {
        pub x: u32,
        pub y: u32,
        pub width: u32,
        pub height: u32,
    }

    /// Composite the camera frame onto the screen frame using the provided overlay configuration.
    pub fn composite_frames(
        screen: &Frame,
        camera: &Frame,
        overlay_config: OverlayConfig,
    ) -> Result<Frame> {
        let mut bg_image =
            ImageBuffer::<Rgba<u8>, _>::from_raw(screen.width, screen.height, screen.data.clone())
                .context("Failed to create background image buffer")?;
        let cam_image =
            ImageBuffer::<Rgba<u8>, _>::from_raw(camera.width, camera.height, camera.data.clone())
                .context("Failed to create camera image buffer")?;
        let cam_dyn = DynamicImage::ImageRgba8(cam_image);
        let resized_cam = cam_dyn.resize_exact(
            overlay_config.width,
            overlay_config.height,
            image::imageops::FilterType::Lanczos3,
        );
        overlay(&mut bg_image, &resized_cam, overlay_config.x, overlay_config.y);
        let final_data = bg_image.into_raw();
        Ok(Frame {
            timestamp: Instant::now(),
            width: screen.width,
            height: screen.height,
            data: final_data,
        })
    }
}

/// Encoder module using GStreamer.
pub mod encoder {
    use super::*;
    use gstreamer::Buffer;
    use gstreamer_app::AppSrc;

    pub struct Encoder {
        appsrc: AppSrc,
    }

    impl Encoder {
        /// Initialize the GStreamer pipeline.
        pub fn new(output_path: &str, width: u32, height: u32, fps: u32) -> Result<Self> {
            gstreamer::init()?;
            let pipeline_description = format!(
                "appsrc name=mysrc ! \
                 videoconvert ! \
                 x264enc speed-preset=ultrafast tune=zerolatency ! \
                 mp4mux ! \
                 filesink location={} sync=false",
                output_path
            );
            let pipeline = gstreamer::parse_launch(&pipeline_description)
                .context("Failed to create GStreamer pipeline")?;

            let appsrc = pipeline
                .clone()
                .dynamic_cast::<gstreamer::Bin>()
                .context("Pipeline is not a Bin")?
                .by_name("mysrc")
                .context("Failed to get appsrc element from pipeline")?
                .dynamic_cast::<AppSrc>()
                .context("Element is not an AppSrc")?;

            appsrc.set_caps(Some(
                &gstreamer::Caps::builder("video/x-raw")
                    .field("format", &"RGBA")
                    .field("width", &(width as i32))
                    .field("height", &(height as i32))
                    .field("framerate", &gstreamer::Fraction::new(fps as i32, 1))
                    .build(),
            ));

            pipeline
                .set_state(gstreamer::State::Playing)
                .context("Unable to set pipeline to Playing state")?;
            println!("Encoder pipeline started, writing to {}", output_path);

            Ok(Self { appsrc })
        }

        /// Push a composite frame into the GStreamer pipeline.
        pub fn push_frame(&self, frame: &Frame) -> Result<()> {
            let buffer_size = (frame.width * frame.height * 4) as usize;
            let mut buffer = Buffer::with_size(buffer_size).context("Failed to create buffer")?;
            {
                let buffer_mut = buffer.get_mut().unwrap();
                let mut map = buffer_mut.map_writable().context("Failed to map buffer")?;
                map.copy_from_slice(&frame.data);
            }
            let pts = frame.timestamp.elapsed().as_nanos() as u64;
            {
                let buffer_mut = buffer.get_mut().unwrap();
                buffer_mut.set_pts(gstreamer::ClockTime::from_nseconds(pts));
                buffer_mut.set_duration(gstreamer::ClockTime::from_seconds(1) / 30);
            }
            self.appsrc
                .push_buffer(buffer)
                .context("Failed to push buffer")?;
            Ok(())
        }
    }
}

/// The recording function that integrates capture, compositing, and encoding.
/// Instead of a fixed-duration loop, it checks the provided stop_flag for graceful shutdown.
pub fn run_recording(stop_flag: Arc<AtomicBool>) -> Result<()> {
    // Setup channels for screen and camera frames.
    let (screen_tx, screen_rx): (Sender<Frame>, Receiver<Frame>) = bounded(5);
    let (camera_tx, camera_rx): (Sender<Frame>, Receiver<Frame>) = bounded(5);

    // Spawn screen capture thread.
    let stop_flag_screen = stop_flag.clone();
    let screen_thread = thread::spawn(move || {
        if let Err(e) = screen_capture::run(screen_tx, stop_flag_screen) {
            eprintln!("Screen capture error: {:?}", e);
        }
    });

    // Spawn camera capture thread.
    let stop_flag_camera = stop_flag.clone();
    let camera_thread = thread::spawn(move || {
        if let Err(e) = camera_capture::run(camera_tx, stop_flag_camera) {
            eprintln!("Camera capture error: {:?}", e);
        }
    });

    // Wait for an initial screen frame to determine video size.
    let initial_screen = screen_rx.recv().context("Failed to receive initial screen frame")?;
    let video_width = initial_screen.width;
    let video_height = initial_screen.height;
    let fps = 30;

    // Initialize encoder.
    let encoder = encoder::Encoder::new("output.mp4", video_width, video_height, fps)?;

    // Shared overlay configuration.
    let overlay_config = Arc::new(Mutex::new(compositor::OverlayConfig {
        x: video_width - 320 - 20, // 20px margin from right
        y: 20,                    // 20px from top
        width: 320,
        height: 240,
    }));

    // Thread to simulate dynamic overlay adjustments.
    {
        let overlay_config = overlay_config.clone();
        let stop_flag_overlay = stop_flag.clone();
        thread::spawn(move || {
            let mut increasing = true;
            while !stop_flag_overlay.load(Ordering::SeqCst) {
                {
                    let mut config = overlay_config.lock().unwrap();
                    if increasing {
                        config.width += 4;
                        config.height += 3;
                        if config.width >= 480 {
                            increasing = false;
                        }
                    } else {
                        if config.width > 320 {
                            config.width -= 4;
                            config.height -= 3;
                        } else {
                            increasing = true;
                        }
                    }
                    config.x = video_width - config.width - 20;
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
    }

    // Main compositing and encoding loop.
    while !stop_flag.load(Ordering::SeqCst) {
        let screen_frame = match screen_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(frame) => frame,
            Err(_) => continue,
        };
        let camera_frame = match camera_rx.try_recv() {
            Ok(frame) => frame,
            Err(_) => continue,
        };

        let config = *overlay_config.lock().unwrap();
        let composite = compositor::composite_frames(&screen_frame, &camera_frame, config)
            .context("Failed to composite frames")?;
        encoder.push_frame(&composite)?;
    }

    println!("Recording stopped. Waiting for threads to finish.");
    let _ = screen_thread.join();
    let _ = camera_thread.join();
    println!("Output file 'output.mp4' generated.");
    Ok(())
}

/// Modified main function: instead of directly starting recording, we launch the modern GUI.
/// The GUI (in src/gui.rs) provides start/stop controls that call `run_recording`.
fn main() -> Result<()> {
    // Launch the GUI application.
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "OBS Recorder",
        native_options,
        Box::new(|_cc| Box::new(gui::RecorderApp::new())),
    );
    Ok(())
}
