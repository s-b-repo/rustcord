use std::sync::mpsc::Receiver;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use pipewire as pw;
use ffmpeg_next as ffmpeg;
use smol::block_on;
use log::{info, error};
use anyhow::Result;

pub enum Command {
    StartRecording(PathBuf),
    StopRecording,
}

pub struct Recorder {
    receiver: Receiver<Command>,
}

impl Recorder {
    pub fn new(receiver: Receiver<Command>) -> Self {
        Self { receiver }
    }

    pub fn run(self) {
        let stop_flag = Arc::new(AtomicBool::new(false));

        while let Ok(command) = self.receiver.recv() {
            match command {
                Command::StartRecording(path) => {
                    info!("Starting recording to {:?}", path);
                    stop_flag.store(false, Ordering::SeqCst);
                    let stop_flag_clone = stop_flag.clone();
                    block_on(async {
                        if let Err(e) = self.start_recording(&path, stop_flag_clone).await {
                            error!("Recording failed: {}", e);
                        }
                    });
                }
                Command::StopRecording => {
                    info!("Stopping recording");
                    stop_flag.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    async fn start_recording(&self, path: &PathBuf, stop_flag: Arc<AtomicBool>) -> Result<()> {
        // Initialize PipeWire
        pw::init();
        let mainloop = pw::MainLoop::new()?;
        let context = pw::Context::new(&mainloop)?;
        let core = context.connect(None)?;

        // Find nodes for video and audio
        let registry = core.get_registry()?;
        let video_node_id = self.find_video_node(&registry, &core)?;
        let audio_node_id = self.find_audio_monitor_node(&registry, &core)?;

        // Initialize FFmpeg
        ffmpeg::init()?;
        let mut output = ffmpeg::format::output(path)?;
        let mut video_stream = output.add_stream(ffmpeg::codec::Id::H264)?;
        let mut audio_stream = output.add_stream(ffmpeg::codec::Id::AAC)?;

        // Configure video stream
        let video_format = self.setup_video_stream(&core, video_node_id, &mut video_stream)?;
        let mut video_encoder = ffmpeg::encoder::video::Video::new(
            video_stream.codec(),
                                                                   video_format.width,
                                                                   video_format.height,
        )?;
        video_encoder.set_format(video_format.pixel_format);
        video_encoder.set_option("preset", "ultrafast")?;
        video_encoder.set_option("crf", "23")?;
        video_encoder.open_as(ffmpeg::codec::Id::H264)?;

        // Configure audio stream
        let audio_format = self.setup_audio_stream(&core, audio_node_id, &mut audio_stream)?;
        let mut audio_encoder = ffmpeg::encoder::audio::Audio::new(
            audio_stream.codec(),
                                                                   audio_format.sample_rate,
                                                                   audio_format.channels,
                                                                   audio_format.sample_format,
        )?;
        audio_encoder.set_bit_rate(128000)?;
        audio_encoder.open_as(ffmpeg::codec::Id::AAC)?;

        output.write_header()?;

        // Set up listeners for processing streams
        let video_listener = self.create_video_listener(&core, video_node_id, &mut video_encoder, &mut output)?;
        let audio_listener = self.create_audio_listener(&core, audio_node_id, &mut audio_encoder, &mut output)?;

        // Run the main loop until stopped
        while !stop_flag.load(Ordering::SeqCst) {
            mainloop.iterate(true)?;
        }

        // Finalize recording
        drop(video_listener);
        drop(audio_listener);
        video_encoder.flush(&mut output)?;
        audio_encoder.flush(&mut output)?;
        output.write_trailer()?;
        info!("Recording saved to {:?}", path);

        unsafe { pw::deinit() };
        Ok(())
    }

    fn find_video_node(&self, registry: &pw::Registry, core: &pw::Core) -> Result<u32> {
        let nodes = registry.list_objects::<pw::Node>();
        for node in nodes {
            if let Some(props) = node.props() {
                if props.get("media.class") == Some("Video/Source") {
                    return Ok(node.id());
                }
            }
        }
        Err(anyhow::anyhow!("No video source node found"))
    }

    fn find_audio_monitor_node(&self, registry: &pw::Registry, core: &pw::Core) -> Result<u32> {
        let nodes = registry.list_objects::<pw::Node>();
        for node in nodes {
            if let Some(props) = node.props() {
                if props.get("media.class") == Some("Audio/Sink") {
                    let sink_name = props.get("node.name").unwrap_or_default();
                    let monitor_name = format!("{}.monitor", sink_name);
                    for monitor_node in nodes.iter() {
                        if monitor_node.props().and_then(|p| p.get("node.name")) == Some(monitor_name.as_str()) {
                            return Ok(monitor_node.id());
                        }
                    }
                }
            }
        }
        Err(anyhow::anyhow!("No audio monitor node found"))
    }

    fn setup_video_stream(
        &self,
        core: &pw::Core,
        node_id: u32,
        stream: &mut ffmpeg::StreamMut,
    ) -> Result<VideoFormat> {
        let video_stream = pw::stream::Stream::new(core, "video-capture", pw::properties! {
            "media.type" => "Video",
            "media.category" => "Capture"
        })?;

        let mut format = None;
        let listener = video_stream.add_listener_local()
        .param_changed(|id, pod| {
            if id == pw::format::PARAM_FORMAT {
                if let Ok(fmt) = pw::format::Format::parse(pod) {
                    if let pw::format::MediaType::Video = fmt.media_type() {
                        if let Some(video) = fmt.video() {
                            format = Some(VideoFormat {
                                width: video.width,
                                height: video.height,
                                pixel_format: ffmpeg::format::Pixel::YUV420P,
                            });
                        }
                    }
                }
            }
        });

        video_stream.connect(
            pw::Direction::Capture,
            Some(node_id),
                             pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        )?;

        while format.is_none() {
            core.sync(0)?;
        }

        let format = format.unwrap();
        stream.set_width(format.width);
        stream.set_height(format.height);
        Ok(format)
    }

    fn setup_audio_stream(
        &self,
        core: &pw::Core,
        node_id: u32,
        stream: &mut ffmpeg::StreamMut,
    ) -> Result<AudioFormat> {
        let audio_stream = pw::stream::Stream::new(core, "audio-capture", pw::properties! {
            "media.type" => "Audio",
            "media.category" => "Capture"
        })?;

        let mut format = None;
        let listener = audio_stream.add_listener_local()
        .param_changed(|id, pod| {
            if id == pw::format::PARAM_FORMAT {
                if let Ok(fmt) = pw::format::Format::parse(pod) {
                    if let pw::format::MediaType::Audio = fmt.media_type() {
                        if let Some(audio) = fmt.audio() {
                            format = Some(AudioFormat {
                                sample_rate: audio.rate,
                                channels: audio.channels as u16,
                                sample_format: ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
                            });
                        }
                    }
                }
            }
        });

        audio_stream.connect(
            pw::Direction::Capture,
            Some(node_id),
                             pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        )?;

        while format.is_none() {
            core.sync(0)?;
        }

        let format = format.unwrap();
        stream.set_rate(format.sample_rate as i32);
        stream.set_channels(format.channels as i32);
        Ok(format)
    }

    fn create_video_listener(
        &self,
        core: &pw::Core,
        node_id: u32,
        encoder: &mut ffmpeg::encoder::video::Video,
        output: &mut ffmpeg::format::context::Output,
    ) -> Result<pw::stream::StreamListenerLocal> {
        let stream = pw::stream::Stream::new(core, "video-capture", pw::properties! {
            "media.type" => "Video",
            "media.category" => "Capture"
        })?;

        let listener = stream.add_listener_local()
        .process(move |_, buffer| {
            if let Some(buffer) = buffer.as_ref() {
                if let Some(data) = buffer.datas().first() {
                    let data = data.as_slice();
                    let mut frame = ffmpeg::frame::Video::new(encoder.format(), encoder.width(), encoder.height());
                    frame.data_mut(0).copy_from_slice(data);
                    frame.set_pts(Some(buffer.pts().unwrap_or(0)));
                    if let Ok(mut packet) = ffmpeg::Packet::empty() {
                        if encoder.encode(&frame, &mut packet).is_ok() {
                            packet.set_stream(0);
                            let _ = output.write_packet(&packet);
                        }
                    }
                }
            }
        });

        stream.connect(
            pw::Direction::Capture,
            Some(node_id),
                       pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        )?;

        Ok(listener)
    }

    fn create_audio_listener(
        &self,
        core: &pw::Core,
        node_id: u32,
        encoder: &mut ffmpeg::encoder::audio::Audio,
        output: &mut ffmpeg::format::context::Output,
    ) -> Result<pw::stream::StreamListenerLocal> {
        let stream = pw::stream::Stream::new(core, "audio-capture", pw::properties! {
            "media.type" => "Audio",
            "media.category" => "Capture"
        })?;

        let listener = stream.add_listener_local()
        .process(move |_, buffer| {
            if let Some(buffer) = buffer.as_ref() {
                if let Some(data) = buffer.datas().first() {
                    let data = data.as_slice();
                    let mut frame = ffmpeg::frame::Audio::new(encoder.format(), encoder.channels(), encoder.rate());
                    frame.data_mut(0).copy_from_slice(data);
                    frame.set_pts(Some(buffer.pts().unwrap_or(0)));
                    if let Ok(mut packet) = ffmpeg::Packet::empty() {
                        if encoder.encode(&frame, &mut packet).is_ok() {
                            packet.set_stream(1);
                            let _ = output.write_packet(&packet);
                        }
                    }
                }
            }
        });

        stream.connect(
            pw::Direction::Capture,
            Some(node_id),
                       pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        )?;

        Ok(listener)
    }
}

struct VideoFormat {
    width: u32,
    height: u32,
    pixel_format: ffmpeg::format::Pixel,
}

struct AudioFormat {
    sample_rate: u32,
    channels: u16,
    sample_format: ffmpeg::format::Sample,
}
