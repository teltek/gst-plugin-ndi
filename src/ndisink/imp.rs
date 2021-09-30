use glib::subclass::prelude::*;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::{gst_debug, gst_error, gst_info, gst_trace};
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;

use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::ndi::SendInstance;

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

#[glib::object_subclass]
impl ObjectSubclass for NdiSink {
    const NAME: &'static str = "NdiSink";
    type Type = super::NdiSink;
    type ParentType = gst_base::BaseSink;

    fn new() -> Self {
        Self {
            settings: Mutex::new(Default::default()),
            state: Mutex::new(Default::default()),
        }
    }
}

impl ObjectImpl for NdiSink {
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
            vec![glib::ParamSpec::new_string(
                "ndi-name",
                "NDI Name",
                "NDI Name to use",
                Some(DEFAULT_SENDER_NDI_NAME.as_ref()),
                glib::ParamFlags::READWRITE,
            )]
        });

        PROPERTIES.as_ref()
    }

    fn set_property(
        &self,
        _obj: &Self::Type,
        _id: usize,
        value: &glib::Value,
        pspec: &glib::ParamSpec,
    ) {
        match pspec.name() {
            "ndi-name" => {
                let mut settings = self.settings.lock().unwrap();
                settings.ndi_name = value
                    .get::<String>()
                    .unwrap_or_else(|_| DEFAULT_SENDER_NDI_NAME.clone());
            }
            _ => unimplemented!(),
        };
    }

    fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "ndi-name" => {
                let settings = self.settings.lock().unwrap();
                settings.ndi_name.to_value()
            }
            _ => unimplemented!(),
        }
    }
}

impl ElementImpl for NdiSink {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "NDI Sink",
                "Sink/Audio/Video",
                "Render as an NDI stream",
                "Sebastian Dröge <sebastian@centricular.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
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
                        .field("format", &gst_audio::AUDIO_FORMAT_F32.to_str())
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
            vec![sink_pad_template]
        });

        PAD_TEMPLATES.as_ref()
    }
}

impl BaseSinkImpl for NdiSink {
    fn start(&self, element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        let mut state_storage = self.state.lock().unwrap();
        let settings = self.settings.lock().unwrap();

        let send = SendInstance::builder(&settings.ndi_name)
            .build()
            .ok_or_else(|| {
                gst::error_msg!(
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

    fn stop(&self, element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        let mut state_storage = self.state.lock().unwrap();

        *state_storage = None;
        gst_info!(CAT, obj: element, "Stopped");

        Ok(())
    }

    fn unlock(&self, _element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        Ok(())
    }

    fn unlock_stop(&self, _element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        Ok(())
    }

    fn set_caps(&self, element: &Self::Type, caps: &gst::Caps) -> Result<(), gst::LoggableError> {
        gst_debug!(CAT, obj: element, "Setting caps {}", caps);

        let mut state_storage = self.state.lock().unwrap();
        let state = match &mut *state_storage {
            None => return Err(gst::loggable_error!(CAT, "Sink not started yet")),
            Some(ref mut state) => state,
        };

        let s = caps.structure(0).unwrap();
        if s.name() == "video/x-raw" {
            let info = gst_video::VideoInfo::from_caps(caps)
                .map_err(|_| gst::loggable_error!(CAT, "Couldn't parse caps {}", caps))?;

            state.video_info = Some(info);
            state.audio_info = None;
        } else {
            let info = gst_audio::AudioInfo::from_caps(caps)
                .map_err(|_| gst::loggable_error!(CAT, "Couldn't parse caps {}", caps))?;

            state.audio_info = Some(info);
            state.video_info = None;
        }

        Ok(())
    }

    fn render(
        &self,
        element: &Self::Type,
        buffer: &gst::Buffer,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let mut state_storage = self.state.lock().unwrap();
        let state = match &mut *state_storage {
            None => return Err(gst::FlowError::Error),
            Some(ref mut state) => state,
        };

        if let Some(ref info) = state.video_info {
            if let Some(audio_meta) = buffer.meta::<crate::ndisinkmeta::NdiSinkAudioMeta>() {
                for (buffer, info, timecode) in audio_meta.buffers() {
                    let frame = crate::ndi::AudioFrame::try_from_buffer(info, buffer, *timecode)
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
                            gst::ClockTime::NONE.display()
                        } else {
                            Some(gst::ClockTime::from_nseconds(*timecode as u64 * 100)).display()
                        },
                        info,
                    );
                    state.send.send_audio(&frame);
                }
            }

            // Skip empty/gap buffers from ndisinkcombiner
            if buffer.size() != 0 {
                let timecode = element
                    .segment()
                    .downcast::<gst::ClockTime>()
                    .ok()
                    .and_then(|segment| {
                        segment
                            .to_running_time(buffer.pts())
                            .zip(element.base_time())
                    })
                    .and_then(|(running_time, base_time)| running_time.checked_add(base_time))
                    .map(|time| (time.nseconds() / 100) as i64)
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
                        gst::ClockTime::NONE.display()
                    } else {
                        Some(gst::ClockTime::from_nseconds(timecode as u64 * 100)).display()
                    },
                    info
                );
                state.send.send_video(&frame);
            }
        } else if let Some(ref info) = state.audio_info {
            let timecode = element
                .segment()
                .downcast::<gst::ClockTime>()
                .ok()
                .and_then(|segment| {
                    segment
                        .to_running_time(buffer.pts())
                        .zip(element.base_time())
                })
                .and_then(|(running_time, base_time)| running_time.checked_add(base_time))
                .map(|time| (time.nseconds() / 100) as i64)
                .unwrap_or(crate::ndisys::NDIlib_send_timecode_synthesize);

            let frame =
                crate::ndi::AudioFrame::try_from_buffer(info, buffer, timecode).map_err(|_| {
                    gst_error!(CAT, obj: element, "Unsupported audio frame");
                    gst::FlowError::NotNegotiated
                })?;

            gst_trace!(
                CAT,
                obj: element,
                "Sending audio buffer {:?} with timecode {} and format {:?}",
                buffer,
                if timecode < 0 {
                    gst::ClockTime::NONE.display()
                } else {
                    Some(gst::ClockTime::from_nseconds(timecode as u64 * 100)).display()
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
