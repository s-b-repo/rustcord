use pipewire as pw;

/// Initialize PipeWire for both screen and audio capture.
pub fn init_pipewire() {
    pw::init();
    println!("PipeWire initialized.");
}
