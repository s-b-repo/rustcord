use gtk4 as gtk;
use gtk::{prelude::*, Orientation};
use libadwaita::{Window as AdwWindow, WindowTitleButtons};
use glib::{clone, MainContext};
use anyhow::Result;
use std::time::Duration;

use crate::core::encoder::{
    get_global_pipeline,
    get_global_scene_switcher,
    get_global_overlay_manager,
    get_global_streaming_manager,
};
use crate::extensions::{
    hardware_accel::setup_unified_hardware_accel,
    streaming::{MultiStreamingManager, StreamingProtocol},
    scene_switcher::SceneSwitcher,
    overlays::OverlayManager,
    plugin_system::{PluginManager, GLOBAL_PLUGIN_MANAGER, load_plugins_from_folder},
};

pub struct SettingsDialog {
    dialog: AdwWindow,
}

impl SettingsDialog {
    pub fn new(parent: &AdwWindow) -> Self {
        let dialog = AdwWindow::new(None);
        dialog.set_title(Some("Advanced Settings"));
        dialog.set_transient_for(Some(parent));
        dialog.set_modal(true);
        dialog.set_default_size(640, 480);

        // Custom header
        let header_bar = gtk::HeaderBar::new();
        header_bar.set_title(Some("Waycord – Advanced Configuration"));
        header_bar.set_show_title_buttons(false);
        let buttons = WindowTitleButtons::new();
        header_bar.pack_end(&buttons);
        dialog.set_titlebar(Some(&header_bar));

        // A stack for pages: Hardware Accel, Scenes, Overlays, Streaming, Plugins
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        stack.set_transition_duration(500);

        let stack_switcher = gtk::StackSwitcher::new();
        stack_switcher.set_stack(Some(&stack));

        let main_box = gtk::Box::new(Orientation::Vertical, 10);
        main_box.pack_start(&stack_switcher, false, false, 0);
        main_box.pack_start(&stack, true, true, 0);

        // -- Hardware Accel Page --
        let hw_box = gtk::Box::new(Orientation::Vertical, 10);
        let hw_label = gtk::Label::new(Some("Configure hardware encoders (VAAPI, NVENC, AMF)."));
        hw_box.append(&hw_label);

        let apply_hw_btn = gtk::Button::with_label("Reapply Hardware Accel");
        hw_box.append(&apply_hw_btn);

        // -- Scenes Page --
        let scene_box = gtk::Box::new(Orientation::Vertical, 10);
        let scene_label = gtk::Label::new(Some("Manage Scenes & Transitions."));
        scene_box.append(&scene_label);

        let fade_btn = gtk::Button::with_label("Fade to Next Scene (1)");
        scene_box.append(&fade_btn);

        // -- Overlays Page --
        let overlay_box = gtk::Box::new(Orientation::Vertical, 10);
        let overlay_label = gtk::Label::new(Some("Add text/images, schedule rotating messages."));
        overlay_box.append(&overlay_label);

        let add_text_btn = gtk::Button::with_label("Add 'Hello' Text Overlay");
        overlay_box.append(&add_text_btn);

        let sponsor_btn = gtk::Button::with_label("Start Sponsor Rotation (hello_overlay)");
        overlay_box.append(&sponsor_btn);

        // -- Streaming Page --
        let streaming_box = gtk::Box::new(Orientation::Vertical, 10);
        let streaming_label = gtk::Label::new(Some("Configure multi-protocol streaming (RTMP/SRT/HLS)."));
        streaming_box.append(&streaming_label);

        let rtmp_entry = gtk::Entry::new();
        rtmp_entry.set_placeholder_text(Some("rtmp://live.twitch.tv/app/xxxxxx"));
        streaming_box.append(&rtmp_entry);

        let add_rtmp_btn = gtk::Button::with_label("Add RTMP Output");
        streaming_box.append(&add_rtmp_btn);

        // -- Plugins Page --
        let plugin_box = gtk::Box::new(Orientation::Vertical, 10);
        let plugin_label = gtk::Label::new(Some("Load dynamic .so plugins for advanced features."));
        plugin_box.append(&plugin_label);

        let plugin_folder_entry = gtk::Entry::new();
        plugin_folder_entry.set_placeholder_text(Some("/usr/lib/waycord-plugins or ~/.local/share/waycord/plugins"));
        plugin_box.append(&plugin_folder_entry);

        let load_plugin_btn = gtk::Button::with_label("Load Plugins from Folder");
        plugin_box.append(&load_plugin_btn);

        // Add pages
        stack.add_titled(&hw_box, Some("hardware"), "Hardware Accel");
        stack.add_titled(&scene_box, Some("scenes"), "Scenes");
        stack.add_titled(&overlay_box, Some("overlays"), "Overlays");
        stack.add_titled(&streaming_box, Some("streaming"), "Streaming");
        stack.add_titled(&plugin_box, Some("plugins"), "Plugins");

        dialog.set_content(Some(&main_box));

        // Retrieve references
        let pipeline_opt = get_global_pipeline();
        let scene_switcher_opt = get_global_scene_switcher();
        let overlay_manager_opt = get_global_overlay_manager();
        let streaming_manager_opt = get_global_streaming_manager();
        let main_context = MainContext::default();

        // (1) Hardware Accel
        apply_hw_btn.connect_clicked(clone!(@weak dialog => move |_| {
            if let Some(ref pipeline) = pipeline_opt {
                let up = pipeline.by_name("videoconvert");
                let down = pipeline.by_name("queue");
                if let (Some(u), Some(d)) = (up, down) {
                    match setup_unified_hardware_accel(pipeline, &u, &d) {
                        Ok(mode) => {
                            println!("Hardware Accel reconfigured: {:?}", mode);
                        },
                        Err(e) => {
                            eprintln!("Failed HW accel setup: {:?}", e);
                        }
                    }
                } else {
                    eprintln!("No upstream 'videoconvert' or downstream 'queue' found in pipeline.");
                }
            }
        }));

        // (2) Scenes
        fade_btn.connect_clicked(move |_| {
            if let Some(switcher) = scene_switcher_opt {
                let _ = switcher.fade_to_scene(1)
                    .map_err(|e| eprintln!("Scene fade error: {:?}", e));
            }
        });

        // (3) Overlays
        add_text_btn.connect_clicked(move |_| {
            if let Some(overlay_mgr) = overlay_manager_opt {
                if let Some(ref pipeline) = pipeline_opt {
                    let up = pipeline.by_name("videoconvert");
                    // We might have software x264 or VAAPI etc
                    let down = pipeline.by_name("vaapi_enc")
                        .or_else(|| pipeline.by_name("nvenc_enc"))
                        .or_else(|| pipeline.by_name("amf_enc"))
                        .or_else(|| pipeline.by_name("soft_x264"));
                    if let (Some(u), Some(d)) = (up, down) {
                        let _ = overlay_mgr.add_text_overlay(
                            "hello_overlay",
                            "Hello from Overlays!",
                            100, 100,
                            "Sans 24",
                            "white",
                            &u,
                            &d
                        ).map_err(|e| eprintln!("Overlay error: {:?}", e));
                    }
                }
            }
        });

        sponsor_btn.connect_clicked(move |_| {
            if let Some(overlay_mgr) = overlay_manager_opt {
                // Sample sponsor messages
                let messages = vec![
                    "Sponsored by Rust!".to_string(),
                    "Waycord: Next-gen screen recorder".to_string(),
                    "Visit example.org for more info".to_string(),
                ];
                let _ = overlay_mgr.start_rotating_messages("hello_overlay", messages, Duration::from_secs(5))
                    .map_err(|e| eprintln!("Sponsor rotation error: {:?}", e));
            }
        });

        // (4) Streaming
        add_rtmp_btn.connect_clicked(clone!(@weak rtmp_entry => move |_| {
            if let Some(manager) = streaming_manager_opt {
                let url = rtmp_entry.text().to_string();
                if !url.is_empty() {
                    let _ = manager.add_output(StreamingProtocol::RTMP(url))
                        .map_err(|e| eprintln!("Add RTMP output error: {:?}", e));
                }
            }
        }));

        // (5) Plugin System
        load_plugin_btn.connect_clicked(clone!(@weak plugin_folder_entry => move |_| {
            let folder = plugin_folder_entry.text().to_string();
            if folder.is_empty() {
                eprintln!("Please provide a folder path for plugins!");
                return;
            }
            // We have a global plugin manager
            unsafe {
                if let Some(ref mut pm) = GLOBAL_PLUGIN_MANAGER {
                    match load_plugins_from_folder(pm, &folder) {
                        Ok(_) => {
                            // Attach to pipeline if needed
                            if let Some(ref pipe) = pipeline_opt {
                                let _ = pm.initialize_all(pipe).map_err(|e| eprintln!("Init plugin error: {:?}", e));
                            }
                            println!("Plugins loaded successfully from '{}'", folder);
                        },
                        Err(e) => {
                            eprintln!("Error loading plugins: {:?}", e);
                        }
                    }
                } else {
                    eprintln!("Global plugin manager not initialized!");
                }
            }
        }));

        Self { dialog }
    }

    pub fn present(&self) {
        self.dialog.present();
    }
}
