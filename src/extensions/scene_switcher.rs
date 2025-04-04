// src/extensions/scene_switcher.rs

use gstreamer as gst;
use gstreamer::prelude::*;
use anyhow::{anyhow, Result};
use glib::MainContext;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct SceneSource {
    /// The element providing the video (e.g. pipewiresrc, v4l2src, test video, etc.)
    pub element: gst::Element,
    /// The compositor sink pad index or name
    pub pad_index: u32,
    /// Current geometry or alpha
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub alpha: f64,
}

/// A single “scene” with multiple sources arranged in a compositor.
pub struct Scene {
    pub name: String,
    pub sources: Vec<SceneSource>,
}

/// SceneSwitcher manages multiple Scenes on a single GStreamer compositor.
/// Transitions can be triggered to fade or wipe from one scene to another.
pub struct SceneSwitcher {
    pipeline: gst::Pipeline,
    compositor: gst::Element,
    scenes: Vec<Scene>,
    current_scene_index: usize,
    transition_duration: Duration,
    // Store a reference to the MainContext or similar for scheduling fade tasks
    main_ctx: MainContext,
    // We keep track of the states of alpha for transitions
    alpha_map: Arc<Mutex<Vec<f64>>>,
}

impl SceneSwitcher {
    /// Provide a reference to the pipeline, a single “compositor” element,
    /// plus an initial set of scenes. Scenes must have their sources added
    /// to the pipeline & linked, but the arrangement is done here.
    /// transition_duration is how long transitions (fade/wipe) last.
    pub fn new(
        pipeline: gst::Pipeline,
        compositor: gst::Element,
        initial_scenes: Vec<Scene>,
        transition_duration: Duration,
        main_ctx: MainContext,
    ) -> Self {
        // Add the compositor to the pipeline
        pipeline.add(&compositor).unwrap();
        // The alpha map will track alpha for each pad. Let's assume an upper bound
        let alpha_map = Arc::new(Mutex::new(vec![1.0; 32])); // up to 32 pads

        Self {
            pipeline,
            compositor,
            scenes: initial_scenes,
            current_scene_index: 0,
            transition_duration,
            main_ctx,
            alpha_map,
        }
    }

    /// Adds a new scene to the switcher. The sources must already be part
    /// of the pipeline (e.g. pipeline.add(&source.element) done externally).
    pub fn add_scene(&mut self, scene: Scene) -> Result<()> {
        self.scenes.push(scene);
        Ok(())
    }

    /// Start in a given scene by index, no transition
    pub fn set_initial_scene(&mut self, index: usize) -> Result<()> {
        if index >= self.scenes.len() {
            return Err(anyhow!("Scene index out of range."));
        }
        self.current_scene_index = index;
        self.apply_scene_layout(index)?;
        Ok(())
    }

    /// Switch from current scene to another scene with a fade transition
    pub fn fade_to_scene(&mut self, new_index: usize) -> Result<()> {
        if new_index >= self.scenes.len() {
            return Err(anyhow!("Scene index out of range."));
        }
        let old_idx = self.current_scene_index;
        if old_idx == new_index {
            return Ok(()); // no-op
        }
        self.current_scene_index = new_index;

        // We'll do a fade out old scene, fade in new scene
        let duration = self.transition_duration;
        self.fade_transition(old_idx, new_index, duration)?;

        println!("Faded from scene {} to scene {}", old_idx, new_index);
        Ok(())
    }

    /// For advanced users: move or resize a source in real-time, e.g. from a GUI.
    pub fn update_source_geometry(
        &mut self,
        scene_index: usize,
        source_pad_index: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<()> {
        if scene_index >= self.scenes.len() {
            return Err(anyhow!("Invalid scene index."));
        }
        let scene = &self.scenes[scene_index];
        // find the matching source
        let maybe_source = scene.sources.iter().find(|s| s.pad_index == source_pad_index);
        if maybe_source.is_none() {
            return Err(anyhow!("No source found with pad_index={} in scene {}", source_pad_index, scene.name));
        }

        // We'll set property on the compositor's sink pad
        let sink_pad_name = format!("sink_{}", source_pad_index);
        let pad = self
            .compositor
            .static_pad(&sink_pad_name)
            .ok_or_else(|| anyhow!("Compositor pad not found: {}", sink_pad_name))?;

        pad.set_property("xpos", x);
        pad.set_property("ypos", y);
        pad.set_property("width", width);
        pad.set_property("height", height);
        Ok(())
    }

    // Internal: apply layout instantly from scene
    fn apply_scene_layout(&self, scene_index: usize) -> Result<()> {
        let scene = &self.scenes[scene_index];
        for src in &scene.sources {
            let pad_name = format!("sink_{}", src.pad_index);
            let pad = self
                .compositor
                .static_pad(&pad_name)
                .ok_or_else(|| anyhow!("Cannot find compositor pad: {}", pad_name))?;

            pad.set_property("xpos", src.x);
            pad.set_property("ypos", src.y);
            pad.set_property("width", src.width);
            pad.set_property("height", src.height);
            pad.set_property("alpha", src.alpha);
        }
        Ok(())
    }

    // Internal: fade out old scene, fade in new scene
    fn fade_transition(&mut self, old_idx: usize, new_idx: usize, dur: Duration) -> Result<()> {
        // Move the new scene's geometry in place, but alpha=0
        let new_scene = &self.scenes[new_idx];
        for src in &new_scene.sources {
            let pad_name = format!("sink_{}", src.pad_index);
            let pad = self
                .compositor
                .static_pad(&pad_name)
                .ok_or_else(|| anyhow!("Cannot find pad for fade in: {}", pad_name))?;
            pad.set_property("xpos", src.x);
            pad.set_property("ypos", src.y);
            pad.set_property("width", src.width);
            pad.set_property("height", src.height);
            pad.set_property("alpha", 0.0);
        }

        // We'll step alpha from 1->0 for old scene, 0->1 for new scene
        // over dur. We'll use a Glib timeout with ~30 fps approach
        let steps = 30;
        let step_time = dur / steps;
        let main_ctx = self.main_ctx.clone();
        let alpha_map = self.alpha_map.clone();
        let compositor_weak = self.compositor.downgrade();
        let old_scene_cloned = self.scenes[old_idx].sources.clone();
        let new_scene_cloned = self.scenes[new_idx].sources.clone();

        main_ctx.spawn_local(async move {
            let mut progress = 0;
            while progress <= steps {
                let frac = progress as f64 / steps as f64;
                let old_alpha = 1.0 - frac;
                let new_alpha = frac;

                if let Some(comp) = compositor_weak.upgrade() {
                    for src in &old_scene_cloned {
                        let pad_name = format!("sink_{}", src.pad_index);
                        if let Some(pad) = comp.static_pad(&pad_name) {
                            pad.set_property("alpha", old_alpha);
                        }
                    }
                    for src in &new_scene_cloned {
                        let pad_name = format!("sink_{}", src.pad_index);
                        if let Some(pad) = comp.static_pad(&pad_name) {
                            pad.set_property("alpha", new_alpha);
                        }
                    }
                }
                progress += 1;
                glib::timeout_future_seconds(step_time.as_secs_f64()).await;
            }
        });

        Ok(())
    }
}
