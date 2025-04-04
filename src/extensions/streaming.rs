// src/extensions/streaming.rs

use gstreamer as gst;
use gstreamer::prelude::*;
use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use glib::MainContext;

/// Represents a single streaming destination (RTMP, SRT, or HLS).
pub enum StreamingProtocol {
    RTMP(String),
    SRT(String),
    HLS(String),
}

/// Each streaming output has its own queue, mux, sink, and possibly new elements.
pub struct StreamingOutput {
    pub protocol: StreamingProtocol,
    /// The set of GStreamer elements used by this output branch
    elements: Vec<gst::Element>,
}

/// Manages multi-protocol streaming, including adaptive bitrate.
pub struct MultiStreamingManager {
    pipeline: gst::Pipeline,
    outputs: Vec<StreamingOutput>,
    // Data for adaptive bitrate
    last_check: Instant,
    bytes_sent: Arc<Mutex<u64>>,
    current_bitrate: u32,
    min_bitrate: u32,
    max_bitrate: u32,
    check_interval: Duration,
}

impl MultiStreamingManager {
    /// Create a new manager with a pipeline reference and initial/min/max bitrates (kbps).
    pub fn new(
        pipeline: gst::Pipeline,
        initial_bitrate: u32,
        min_bitrate: u32,
        max_bitrate: u32,
    ) -> Self {
        Self {
            pipeline,
            outputs: Vec::new(),
            last_check: Instant::now(),
            bytes_sent: Arc::new(Mutex::new(0)),
            current_bitrate: initial_bitrate,
            min_bitrate,
            max_bitrate,
            check_interval: Duration::from_secs(5), // check every 5 seconds
        }
    }

    /// Add a streaming destination (RTMP, SRT, or HLS) to the pipeline.
    pub fn add_output(&mut self, protocol: StreamingProtocol) -> Result<()> {
        // Build queue + mux + sink based on protocol
        let queue = gst::ElementFactory::make("queue", None)?;
        let mux: gst::Element;
        let sink: gst::Element;

        match &protocol {
            StreamingProtocol::RTMP(loc) => {
                mux = gst::ElementFactory::make("flvmux", None)?;
                sink = gst::ElementFactory::make("rtmpsink", None)?;
                sink.set_property("location", loc);
            }
            StreamingProtocol::SRT(loc) => {
                // SRT streaming typically uses mpegtsmux
                mux = gst::ElementFactory::make("mpegtsmux", None)?;
                sink = gst::ElementFactory::make("srtsink", None)?;
                sink.set_property("uri", loc);
            }
            StreamingProtocol::HLS(dir) => {
                // Basic HLS example using hlssink
                mux = gst::ElementFactory::make("mpegtsmux", None)?;
                sink = gst::ElementFactory::make("hlssink", None)?;
                sink.set_property("location", format!("{}/segment_%05d.ts", dir));
                sink.set_property("playlist-location", format!("{}/playlist.m3u8", dir));
            }
        }

        // Add to pipeline
        self.pipeline.add_many(&[&queue, &mux, &sink])?;
        queue.sync_state_with_parent()?;
        mux.sync_state_with_parent()?;
        sink.sync_state_with_parent()?;

        // Link queue -> mux -> sink
        gst::Element::link_many(&[&queue, &mux, &sink])
            .map_err(|_| anyhow!("Failed to link streaming elements for output."))?;

        // Store them
        self.outputs.push(StreamingOutput {
            protocol,
            elements: vec![queue, mux, sink],
        });

        Ok(())
    }

    /// Link this streaming manager to the encoder’s output. Typically you'd do:
    ///   x264enc ! tee name=t ! queue ! multi_stream_manager
    /// So it has a single input pad from the tee or queue.
    pub fn link_input(&mut self, src_element: &gst::Element) -> Result<()> {
        // We link from src_element’s src pad to each output queue’s sink pad
        for output in &self.outputs {
            // First element in output branch is the queue
            let queue = &output.elements[0];
            gst::Element::link_many(&[src_element, queue])
                .map_err(|_| anyhow!("Failed to link src_element to streaming queue."))?;
        }
        Ok(())
    }

    /// Start adaptive bitrate monitoring. We'll track data usage every X seconds,
    /// adjust the bitrate property on the encoder if needed.
    pub fn start_adaptive_bitrate(
        &mut self,
        encoder: &gst::Element,
        main_ctx: &MainContext,
    ) -> Result<()> {
        let encoder_weak = encoder.downgrade();
        let bytes_sent_arc = self.bytes_sent.clone();
        let interval = self.check_interval;
        let min_br = self.min_bitrate;
        let max_br = self.max_bitrate;

        // The GStreamer pipeline’s bus can be used to monitor for messages (like EOS).
        // For simplicity, we track data on an element’s “bytes” property if available,
        // or we can hook into a signal. We'll do a simple custom approach.
        //
        // This example uses a Glib timeout. We gather the current bytes, compare to last check,
        // compute “effective bitrate,” and adjust up/down.
        let mut last_time = self.last_check;
        let mut last_bytes: u64 = 0;
        let mut current_bitrate = self.current_bitrate; // kbps
        main_ctx.spawn_local(glib::timeout_future_seconds(interval.as_secs_f64()).then(move |()| async move {
            // We schedule ourselves repeatedly
            loop {
                let now = Instant::now();
                let elapsed_secs = now.duration_since(last_time).as_secs_f64();

                let bytes_sent_now = *bytes_sent_arc.lock().unwrap();
                let bytes_delta = bytes_sent_now.saturating_sub(last_bytes);
                // convert to Kbits/s
                let kbits_per_sec = (bytes_delta as f64 * 8.0 / 1024.0) / elapsed_secs;

                if let Some(enc) = encoder_weak.upgrade() {
                    // If the measured kbits/s is near current bitrate, we might bump or reduce
                    // For demonstration, let's do a naive approach:
                    // if usage < 70% of currentBitrate => we can raise it
                    // if usage > 95% => we lower it
                    if kbits_per_sec < current_bitrate as f64 * 0.7 {
                        current_bitrate = (current_bitrate + 256).min(max_br);
                        enc.set_property("bitrate", current_bitrate);
                        println!("[Adaptive] Increased bitrate to {} kbps.", current_bitrate);
                    } else if kbits_per_sec > current_bitrate as f64 * 0.95 {
                        current_bitrate = (current_bitrate.saturating_sub(256)).max(min_br);
                        enc.set_property("bitrate", current_bitrate);
                        println!("[Adaptive] Decreased bitrate to {} kbps.", current_bitrate);
                    }
                }
                last_bytes = bytes_sent_now;
                last_time = now;

                glib::timeout_future_seconds(interval.as_secs_f64()).await;
            }
        }));

        println!("Adaptive Bitrate monitoring started.");
        Ok(())
    }

    /// Call this whenever new data is sent. For instance, attach to a probe
    /// on the final streaming pad or each sink's “handoff.”
    /// This increments a shared bytes counter for adaptive logic.
    pub fn record_bytes_sent(&self, count: usize) {
        let mut total = self.bytes_sent.lock().unwrap();
        *total += count as u64;
    }
}
