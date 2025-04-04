// src/extensions/overlays.rs

use gstreamer as gst;
use gstreamer::prelude::*;
use anyhow::{anyhow, Result};
use glib::MainContext;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct OverlayManager {
    pipeline: gst::Pipeline,
    text_overlays: Vec<gst::Element>,
    image_overlays: Vec<gst::Element>,
    main_ctx: MainContext,
    // Sponsor or rotating messages
    rotating_msgs: Arc<Mutex<Vec<String>>>,
    current_msg_index: Arc<Mutex<usize>>,
}

impl OverlayManager {
    pub fn new(pipeline: gst::Pipeline, main_ctx: MainContext) -> Self {
        Self {
            pipeline,
            text_overlays: Vec::new(),
            image_overlays: Vec::new(),
            main_ctx,
            rotating_msgs: Arc::new(Mutex::new(Vec::new())),
            current_msg_index: Arc::new(Mutex::new(0)),
        }
    }

    /// Add a text overlay at given coordinates with a given message.
    /// This creates a `textoverlay` element, adds it to the pipeline,
    /// and attempts to link it somewhere in the chain.
    pub fn add_text_overlay(
        &mut self,
        name: &str,
        message: &str,
        x: i32,
        y: i32,
        font_desc: &str,
        color: &str,
        upstream: &gst::Element,
        downstream: &gst::Element,
    ) -> Result<()> {
        let overlay = gst::ElementFactory::make("textoverlay", Some(name))
            .map_err(|_| anyhow!("Failed to create textoverlay. Is plugin installed?"))?;
        overlay.set_property("text", message);
        overlay.set_property("font-desc", font_desc);
        overlay.set_property("color", color);
        overlay.set_property("valignment", "top");
        overlay.set_property("halignment", "left");
        // The “xpos” and “ypos” are ratio-based for textoverlay's "shaded-background" or overlay geometry
        // For exact pixel positions, we might need different elements. We'll do best effort:
        overlay.set_property("xpos", x);
        overlay.set_property("ypos", y);

        self.pipeline.add(&overlay)?;
        overlay.sync_state_with_parent()?;

        // Link: upstream -> overlay -> downstream
        gst::Element::link_many(&[upstream, &overlay, downstream])
            .map_err(|_| anyhow!("Failed to link text overlay."))?;

        self.text_overlays.push(overlay);
        Ok(())
    }

    /// Add an image overlay (like a watermark) at a given position.
    /// We use "gdkpixbufoverlay" or "pngalpha" pipeline approach if needed.
    /// This example uses "gdkpixbufoverlay".
    pub fn add_image_overlay(
        &mut self,
        name: &str,
        image_path: &str,
        x: i32,
        y: i32,
        upstream: &gst::Element,
        downstream: &gst::Element,
    ) -> Result<()> {
        let overlay = gst::ElementFactory::make("gdkpixbufoverlay", Some(name))
            .map_err(|_| anyhow!("Failed to create gdkpixbufoverlay. Is plugin installed?"))?;
        overlay.set_property("location", image_path);
        overlay.set_property("offset-x", x);
        overlay.set_property("offset-y", y);

        self.pipeline.add(&overlay)?;
        overlay.sync_state_with_parent()?;

        gst::Element::link_many(&[upstream, &overlay, downstream])
            .map_err(|_| anyhow!("Failed to link image overlay."))?;

        self.image_overlays.push(overlay);
        Ok(())
    }

    /// Let the user add multiple rotating sponsor messages. This function starts
    /// a timer that updates the text property of a specified textoverlay element
    /// every N seconds.
    pub fn start_rotating_messages(
        &mut self,
        textoverlay_name: &str,
        messages: Vec<String>,
        interval: Duration,
    ) -> Result<()> {
        if messages.is_empty() {
            return Err(anyhow!("No messages provided for rotation."));
        }
        // store messages
        {
            let mut vec_ref = self.rotating_msgs.lock().unwrap();
            *vec_ref = messages;
        }
        let textoverlay = self
            .pipeline
            .by_name(textoverlay_name)
            .ok_or_else(|| anyhow!("Text overlay {} not found in pipeline.", textoverlay_name))?;

        let rotating_msgs = self.rotating_msgs.clone();
        let index_ref = self.current_msg_index.clone();
        let overlay_weak = textoverlay.downgrade();
        let mc = self.main_ctx.clone();

        mc.spawn_local(async move {
            loop {
                // Wait interval
                glib::timeout_future_seconds(interval.as_secs_f64()).await;

                let msgs = rotating_msgs.lock().unwrap();
                let mut idx = index_ref.lock().unwrap();
                if msgs.is_empty() {
                    continue;
                }

                let message = &msgs[*idx];
                if let Some(overlay) = overlay_weak.upgrade() {
                    overlay.set_property("text", message.as_str());
                }
                *idx = (*idx + 1) % msgs.len();
            }
        });

        Ok(())
    }
}
