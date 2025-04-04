use pipewire::main_loop::MainLoop;
use pipewire::context::Context as PwContext;
use pipewire::properties::Properties;
use pipewire::registry::{Registry, RegistryEvents};
use pipewire::spa::pod::Pod;
use pipewire::types::ObjectType;
use std::cell::RefCell;
use std::rc::Rc;

pub fn get_audio_sources() -> Vec<(u32, String, String)> {
    pipewire::init();

    let main_loop = MainLoop::new().expect("Failed to create PipeWire MainLoop");
    let context = PwContext::new(&main_loop, Properties::new().unwrap())
        .expect("Failed to create PipeWire context");
    let core = context.connect(None).expect("Failed to connect to PipeWire core");
    let registry = core.get_registry().expect("Failed to get PipeWire registry");

    let audio_sources = Rc::new(RefCell::new(Vec::new()));
    let audio_sources_clone = audio_sources.clone();

    registry.bind(&RegistryEvents {
        global: Some(Box::new(move |id, _permissions, obj_type, _version, properties| {
            if obj_type != ObjectType::Node {
                return;
            }
            let media_class = properties.get("media.class").unwrap_or("");
            if media_class == "Audio/Source" || media_class == "Audio/Source/Virtual" {
                let name = properties.get("node.name").unwrap_or("Unknown");
                let desc = properties.get("node.description").unwrap_or("No description");
                audio_sources_clone.borrow_mut().push((id, name.to_string(), desc.to_string()));
            }
        })),
        ..Default::default()
    });

    main_loop.iterate(false);
    audio_sources.borrow().clone()
}
