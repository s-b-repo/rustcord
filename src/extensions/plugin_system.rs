use anyhow::{Result, anyhow};
use gstreamer as gst;
use gstreamer::prelude::*;
use libloading::{Library, Symbol};
use std::fs;
use std::path::PathBuf;
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub trait WaycordPlugin {
    fn on_load(&self) -> Result<()>;
    fn attach_to_pipeline(&self, pipeline: &gst::Pipeline) -> Result<()>;
}

type InitFn = unsafe fn() -> *mut dyn WaycordPlugin;

/// A loaded plugin – keeps a ref to the dynamic library plus the plugin instance.
pub struct DynamicPlugin {
    _library: Library,
    instance: Box<dyn WaycordPlugin>,
}

impl DynamicPlugin {
    pub fn load_from_file(path: &str) -> Result<Self> {
        let lib = unsafe { Library::new(path) }
            .map_err(|e| anyhow!("Failed to load plugin {}: {}", path, e))?;
        let func: Symbol<InitFn> = unsafe {
            lib.get(b"waycord_plugin_init\0")
                .map_err(|e| anyhow!("Missing init symbol in {}: {}", path, e))?
        };
        let plugin_ptr = unsafe { func() };
        if plugin_ptr.is_null() {
            return Err(anyhow!("Plugin init returned null pointer."));
        }
        let instance = unsafe { Box::from_raw(plugin_ptr) };

        Ok(Self {
            _library: lib,
            instance,
        })
    }

    pub fn plugin(&self) -> &dyn WaycordPlugin {
        self.instance.as_ref()
    }
}

/// Manages a list of dynamic (and/or static) plugins
pub struct PluginManager {
    dynamic_plugins: Vec<DynamicPlugin>,
    // If you want static plugins implementing the trait, store them here as well
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            dynamic_plugins: Vec::new(),
        }
    }

    pub fn load_plugin_file(&mut self, path: &str) -> Result<()> {
        let dp = DynamicPlugin::load_from_file(path)?;
        dp.plugin().on_load()?;
        self.dynamic_plugins.push(dp);
        Ok(())
    }

    pub fn initialize_all(&self, pipeline: &gst::Pipeline) -> Result<()> {
        for dp in &self.dynamic_plugins {
            dp.plugin().attach_to_pipeline(pipeline)?;
        }
        Ok(())
    }
}

/// A global plugin manager for the entire app
pub static mut GLOBAL_PLUGIN_MANAGER: Option<PluginManager> = None;

/// Load all `.so` plugins from a given folder
pub fn load_plugins_from_folder(manager: &mut PluginManager, folder: &str) -> Result<()> {
    let p = PathBuf::from(folder);
    if !p.exists() {
        return Err(anyhow!("Plugin folder does not exist: {}", folder));
    }

    for entry in fs::read_dir(p)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|x| x == "so").unwrap_or(false) {
            let path_str = path.to_string_lossy().to_string();
            manager.load_plugin_file(&path_str)?;
        }
    }
    Ok(())
}
