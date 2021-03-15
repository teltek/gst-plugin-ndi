use glib::subclass;
use glib::subclass::prelude::*;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::{gst_debug, gst_error, gst_error_msg, gst_info, gst_loggable_error, gst_trace};
use gst_base::{subclass::prelude::*, BaseSinkExtManual};

use std::sync::Mutex;

use once_cell::sync::Lazy;

use super::ndi::SendInstance;

static DEFAULT_SENDER_NDI_NAME: Lazy<String> = Lazy::new(|| {
    format!(
        "GStreamer NDI Sink {}-{}",
        env!("CARGO_PKG_VERSION"),
        env!("COMMIT_ID")
    )
});

#[derive(Debug)]
struct Settings {
    ndi_name: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ndi_name: DEFAULT_SENDER_NDI_NAME.clone(),
        }
    }
}

static PROPERTIES: [subclass::Property; 1] = [subclass::Property("ndi-name", |name| {
    glib::ParamSpec::string(
        name,
        "NDI Name",
        "NDI Name to use",
        Some(DEFAULT_SENDER_NDI_NAME.as_ref()),
        glib::ParamFlags::READWRITE,
    )
})];

struct State {
    send: SendInstance,
    video_info: Option<gst_video::VideoInfo>,
    audio_info: Option<gst_audio::AudioInfo>,
}

pub struct NdiSink {
    settings: Mutex<Settings>,
    state: Mutex<Option<State>>,
}

static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new("ndisink", gst::DebugColorFlags::empty(), Some("NDI Sink"))
});

impl ObjectSubclass for NdiSink {
    const NAME: &'static str = "NdiSink";
    type ParentType = gst_base::BaseSink;
    type Instance = gst::subclass::ElementInstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib::glib_object_subclass!();

    fn new() -> Self {
        Self {
            settings: Mutex::new(Default::default()),
            state: Mutex::new(Default::default()),
        }
    }

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.set_metadata(
            "NDI Sink",
            "Sink/Audio/Video",
            "Render as an NDI stream",
            "Sebastian Dröge <sebastian@centricular.com>",
        );

        let caps = gst::Caps::builder_full()
            .structure(
                gst::Structure::builder("video/x-raw")
                    .field(
                        "format",
                        &gst::List::new(&[
                            &gst_video::VideoFormat::Uyvy.to_str(),
                            &gst_video::VideoFormat::I420.to_str(),
                            &gst_video::VideoFormat::Nv12.to_str(),
                            &gst_video::VideoFormat::Nv21.to_str(),
                            &gst_video::VideoFormat::Yv12.to_str(),
                            &gst_video::VideoFormat::Bgra.to_str(),
                            &gst_video::VideoFormat::Bgrx.to_str(),
                            &gst_video::VideoFormat::Rgba.to_str(),
                            &gst_video::VideoFormat::Rgbx.to_str(),
                        ]),
                    )
                    .field("width", &gst::IntRange::<i32>::new(1, std::i32::MAX))
                    .field("height", &gst::IntRange::<i32>::new(1, std::i32::MAX))
                    .field(
                        "framerate",
                        &gst::FractionRange::new(
                            gst::Fraction::new(0, 1),
                            gst::Fraction::new(std::i32::MAX, 1),
                        ),
                    )
                    .build(),
            )
            .structure(
                gst::Structure::builder("audio/x-raw")
                    .field("format", &gst_audio::AUDIO_FORMAT_S16.to_str())
                    .field("rate", &gst::IntRange::<i32>::new(1, i32::MAX))
                    .field("channels", &gst::IntRange::<i32>::new(1, i32::MAX))
                    .field("layout", &"interleaved")
                    .build(),
            )
            .build();

        let sink_pad_template = gst::PadTemplate::new(
            "sink",
            gst::PadDirection::Sink,
            gst::PadPresence::Always,
            &caps,
        )
        .unwrap();
        klass.add_pad_template(sink_pad_template);

        klass.install_properties(&PROPERTIES);
    }
}

impl ObjectImpl for NdiSink {
    glib::glib_object_impl!();

    fn set_property(&self, _obj: &glib::Object, id: usize, value: &glib::Value) {
        let prop = &PROPERTIES[id];
        match *prop {
            subclass::Property("ndi-name", ..) => {
                let mut settings = self.settings.lock().unwrap();
                settings.ndi_name = value
                    .get::<String>()
                    .unwrap()
                    .unwrap_or_else(|| DEFAULT_SENDER_NDI_NAME.clone());
            }
            _ => unimplemented!(),
        };
    }

    fn get_property(&self, _obj: &glib::Object, id: usize) -> Result<glib::Value, ()> {
        let prop = &PROPERTIES[id];
        match *prop {
            subclass::Property("ndi-name", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.ndi_name.to_value())
            }
            _ => unimplemented!(),
        }
    }
}

impl ElementImpl for NdiSink {}

impl BaseSinkImpl for NdiSink {
    fn start(&self, element: &gst_base::BaseSink) -> Result<(), gst::ErrorMessage> {
        let mut state_storage = self.state.lock().unwrap();
        let settings = self.settings.lock().unwrap();

        let send = SendInstance::builder(&settings.ndi_name)
            .build()
            .ok_or_else(|| {
                gst_error_msg!(
                    gst::ResourceError::OpenWrite,
                    ["Could not create send instance"]
                )
            })?;

        let state = State {
            send,
            video_info: None,
            audio_info: None,
        };
        *state_storage = Some(state);
        gst_info!(CAT, obj: element, "Started");

        Ok(())
    }

    fn stop(&self, element: &gst_base::BaseSink) -> Result<(), gst::ErrorMessage> {
        let mut state_storage = self.state.lock().unwrap();

        *state_storage = None;
        gst_info!(CAT, obj: element, "Stopped");

        Ok(())
    }

    fn unlock(&self, _element: &gst_base::BaseSink) -> Result<(), gst::ErrorMessage> {
        Ok(())
    }

    fn unlock_stop(&self, _element: &gst_base::BaseSink) -> Result<(), gst::ErrorMessage> {
        Ok(())
    }

    fn set_caps(
        &self,
        element: &gst_base::BaseSink,
        caps: &gst::Caps,
    ) -> Result<(), gst::LoggableError> {
        gst_debug!(CAT, obj: element, "Setting caps {}", caps);

        let mut state_storage = self.state.lock().unwrap();
        let state = match &mut *state_storage {
            None => return Err(gst_loggable_error!(CAT, "Sink not started yet")),
            Some(ref mut state) => state,
        };

        let s = caps.get_structure(0).unwrap();
        if s.get_name() == "video/x-raw" {
            let info = gst_video::VideoInfo::from_caps(caps)
                .map_err(|_| gst_loggable_error!(CAT, "Couldn't parse caps {}", caps))?;

            state.video_info = Some(info);
            state.audio_info = None;
        } else {
            let info = gst_audio::AudioInfo::from_caps(caps)
                .map_err(|_| gst_loggable_error!(CAT, "Couldn't parse caps {}", caps))?;

            state.audio_info = Some(info);
            state.video_info = None;
        }

        Ok(())
    }

    fn render(
        &self,
        element: &gst_base::BaseSink,
        buffer: &gst::Buffer,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state_storage = self.state.lock().unwrap();
        let state = match &mut *state_storage {
            None => return Err(gst::FlowError::Error),
            Some(ref mut state) => state,
        };

        if let Some(ref info) = state.video_info {
            if let Some(audio_meta) = buffer.get_meta::<crate::ndisinkmeta::NdiSinkAudioMeta>() {
                for (buffer, info, timecode) in audio_meta.buffers() {
                    let frame =
                        crate::ndi::AudioFrame::try_from_interleaved_16s(info, buffer, *timecode)
                            .map_err(|_| {
                            gst_error!(CAT, obj: element, "Unsupported audio frame");
                            gst::FlowError::NotNegotiated
                        })?;

                    gst_trace!(
                        CAT,
                        obj: element,
                        "Sending audio buffer {:?} with timecode {} and format {:?}",
                        buffer,
                        if *timecode < 0 {
                            gst::CLOCK_TIME_NONE
                        } else {
                            gst::ClockTime::from(*timecode as u64 * 100)
                        },
                        info,
                    );
                    state.send.send_audio(&frame);
                }
            }

            // Skip empty/gap buffers from ndisinkcombiner
            if buffer.get_size() != 0 {
                let timecode = element
                    .get_segment()
                    .downcast::<gst::ClockTime>()
                    .ok()
                    .and_then(|segment| {
                        *(segment.to_running_time(buffer.get_pts()) + element.get_base_time())
                    })
                    .map(|time| (time / 100) as i64)
                    .unwrap_or(crate::ndisys::NDIlib_send_timecode_synthesize);

                let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, info)
                    .map_err(|_| {
                        gst_error!(CAT, obj: element, "Failed to map buffer");
                        gst::FlowError::Error
                    })?;

                let frame = crate::ndi::VideoFrame::try_from_video_frame(&frame, timecode)
                    .map_err(|_| {
                        gst_error!(CAT, obj: element, "Unsupported video frame");
                        gst::FlowError::NotNegotiated
                    })?;

                gst_trace!(
                    CAT,
                    obj: element,
                    "Sending video buffer {:?} with timecode {} and format {:?}",
                    buffer,
                    if timecode < 0 {
                        gst::CLOCK_TIME_NONE
                    } else {
                        gst::ClockTime::from(timecode as u64 * 100)
                    },
                    info
                );
                state.send.send_video(&frame);
            }
        } else if let Some(ref info) = state.audio_info {
            let timecode = element
                .get_segment()
                .downcast::<gst::ClockTime>()
                .ok()
                .and_then(|segment| {
                    *(segment.to_running_time(buffer.get_pts()) + element.get_base_time())
                })
                .map(|time| (time / 100) as i64)
                .unwrap_or(crate::ndisys::NDIlib_send_timecode_synthesize);

            let frame = crate::ndi::AudioFrame::try_from_interleaved_16s(info, buffer, timecode)
                .map_err(|_| {
                    gst_error!(CAT, obj: element, "Unsupported audio frame");
                    gst::FlowError::NotNegotiated
                })?;

            gst_trace!(
                CAT,
                obj: element,
                "Sending audio buffer {:?} with timecode {} and format {:?}",
                buffer,
                if timecode < 0 {
                    gst::CLOCK_TIME_NONE
                } else {
                    gst::ClockTime::from(timecode as u64 * 100)
                },
                info,
            );
            state.send.send_audio(&frame);
        } else {
            return Err(gst::FlowError::Error);
        }

        Ok(gst::FlowSuccess::Ok)
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndisink",
        gst::Rank::None,
        NdiSink::get_type(),
    )
}
