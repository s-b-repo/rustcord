use gtk4::{Box as GtkBox, Orientation};
use gtk4::prelude::*;
use gstreamer as gst;
use gstreamer::prelude::*;

pub fn build_preview() -> GtkBox {
    gst::init().unwrap();

    let container = GtkBox::new(Orientation::Vertical, 5);
    let preview_box = GtkBox::new(Orientation::Vertical, 0);
    container.append(&preview_box);

    let pipeline_str = "pipewiresrc ! videoconvert ! queue ! gtksink name=sink";
    let pipeline = gst::parse_launch(pipeline_str).expect("Failed to create preview pipeline");
    let pipeline = pipeline.downcast::<gst::Pipeline>().unwrap();

    let sink = pipeline.by_name("sink").expect("Failed to find gtksink");
    if let Ok(Some(sink_widget)) = sink.property::<Option<gtk4::Widget>>("widget") {
        preview_box.append(&sink_widget);
        sink_widget.set_size_request(1280, 720);
    }

    pipeline.set_state(gst::State::Playing).unwrap();

    container
}
