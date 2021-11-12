use glib::prelude::*;
use glib::subclass::prelude::*;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::{gst_debug, gst_error, gst_trace, gst_warning};
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;

use once_cell::sync::Lazy;

use std::mem;
use std::sync::Mutex;

static CAT: once_cell::sync::Lazy<gst::DebugCategory> = once_cell::sync::Lazy::new(|| {
    gst::DebugCategory::new(
        "ndisinkcombiner",
        gst::DebugColorFlags::empty(),
        Some("NDI sink audio/video combiner"),
    )
});

struct State {
    // Note that this applies to the currently pending buffer on the pad and *not*
    // to the current_video_buffer below!
    video_info: Option<gst_video::VideoInfo>,
    audio_info: Option<gst_audio::AudioInfo>,
    current_video_buffer: Option<(gst::Buffer, gst::ClockTime)>,
    current_audio_buffers: Vec<(gst::Buffer, gst_audio::AudioInfo, i64)>,
}

pub struct NdiSinkCombiner {
    video_pad: gst_base::AggregatorPad,
    audio_pad: Mutex<Option<gst_base::AggregatorPad>>,
    state: Mutex<Option<State>>,
}

#[glib::object_subclass]
impl ObjectSubclass for NdiSinkCombiner {
    const NAME: &'static str = "NdiSinkCombiner";
    type Type = super::NdiSinkCombiner;
    type ParentType = gst_base::Aggregator;

    fn with_class(klass: &Self::Class) -> Self {
        let templ = klass.pad_template("video").unwrap();
        let video_pad =
            gst::PadBuilder::<gst_base::AggregatorPad>::from_template(&templ, Some("video"))
                .build();

        Self {
            video_pad,
            audio_pad: Mutex::new(None),
            state: Mutex::new(None),
        }
    }
}

impl ObjectImpl for NdiSinkCombiner {
    fn constructed(&self, obj: &Self::Type) {
        obj.add_pad(&self.video_pad).unwrap();

        self.parent_constructed(obj);
    }
}

impl ElementImpl for NdiSinkCombiner {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "NDI Sink Combiner",
                "Combiner/Audio/Video",
                "NDI sink audio/video combiner",
                "Sebastian Dröge <sebastian@centricular.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
            let caps = gst::Caps::builder("video/x-raw")
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
                .field("width", &gst::IntRange::<i32>::new(1, i32::MAX))
                .field("height", &gst::IntRange::<i32>::new(1, i32::MAX))
                .field(
                    "framerate",
                    &gst::FractionRange::new(
                        gst::Fraction::new(1, i32::MAX),
                        gst::Fraction::new(i32::MAX, 1),
                    ),
                )
                .build();
            let src_pad_template = gst::PadTemplate::with_gtype(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &caps,
                gst_base::AggregatorPad::static_type(),
            )
            .unwrap();

            let video_sink_pad_template = gst::PadTemplate::with_gtype(
                "video",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &caps,
                gst_base::AggregatorPad::static_type(),
            )
            .unwrap();

            let caps = gst::Caps::builder("audio/x-raw")
                .field("format", &gst_audio::AUDIO_FORMAT_F32.to_str())
                .field("rate", &gst::IntRange::<i32>::new(1, i32::MAX))
                .field("channels", &gst::IntRange::<i32>::new(1, i32::MAX))
                .field("layout", &"interleaved")
                .build();
            let audio_sink_pad_template = gst::PadTemplate::with_gtype(
                "audio",
                gst::PadDirection::Sink,
                gst::PadPresence::Request,
                &caps,
                gst_base::AggregatorPad::static_type(),
            )
            .unwrap();
            vec![
                src_pad_template,
                video_sink_pad_template,
                audio_sink_pad_template,
            ]
        });

        PAD_TEMPLATES.as_ref()
    }

    fn release_pad(&self, element: &Self::Type, pad: &gst::Pad) {
        let mut audio_pad_storage = self.audio_pad.lock().unwrap();

        if audio_pad_storage.as_ref().map(|p| p.upcast_ref()) == Some(pad) {
            gst_debug!(CAT, obj: element, "Release audio pad");
            self.parent_release_pad(element, pad);
            *audio_pad_storage = None;
        }
    }
}

impl AggregatorImpl for NdiSinkCombiner {
    fn create_new_pad(
        &self,
        agg: &Self::Type,
        templ: &gst::PadTemplate,
        _req_name: Option<&str>,
        _caps: Option<&gst::Caps>,
    ) -> Option<gst_base::AggregatorPad> {
        let mut audio_pad_storage = self.audio_pad.lock().unwrap();

        if audio_pad_storage.is_some() {
            gst_error!(CAT, obj: agg, "Audio pad already requested");
            return None;
        }

        let sink_templ = agg.pad_template("audio").unwrap();
        if templ != &sink_templ {
            gst_error!(CAT, obj: agg, "Wrong pad template");
            return None;
        }

        let pad =
            gst::PadBuilder::<gst_base::AggregatorPad>::from_template(templ, Some("audio")).build();
        *audio_pad_storage = Some(pad.clone());

        gst_debug!(CAT, obj: agg, "Requested audio pad");

        Some(pad)
    }

    fn start(&self, agg: &Self::Type) -> Result<(), gst::ErrorMessage> {
        let mut state_storage = self.state.lock().unwrap();
        *state_storage = Some(State {
            audio_info: None,
            video_info: None,
            current_video_buffer: None,
            current_audio_buffers: Vec::new(),
        });

        gst_debug!(CAT, obj: agg, "Started");

        Ok(())
    }

    fn stop(&self, agg: &Self::Type) -> Result<(), gst::ErrorMessage> {
        // Drop our state now
        let _ = self.state.lock().unwrap().take();

        gst_debug!(CAT, obj: agg, "Stopped");

        Ok(())
    }

    fn next_time(&self, _agg: &Self::Type) -> Option<gst::ClockTime> {
        // FIXME: What to do here? We don't really know when the next buffer is expected
        gst::ClockTime::NONE
    }

    fn clip(
        &self,
        agg: &Self::Type,
        agg_pad: &gst_base::AggregatorPad,
        mut buffer: gst::Buffer,
    ) -> Option<gst::Buffer> {
        let segment = match agg_pad.segment().downcast::<gst::ClockTime>() {
            Ok(segment) => segment,
            Err(_) => {
                gst_error!(CAT, obj: agg, "Only TIME segments supported");
                return Some(buffer);
            }
        };

        let pts = buffer.pts();
        if pts.is_none() {
            gst_error!(CAT, obj: agg, "Only buffers with PTS supported");
            return Some(buffer);
        }

        let duration = buffer.duration();

        gst_trace!(
            CAT,
            obj: agg_pad,
            "Clipping buffer {:?} with PTS {} and duration {}",
            buffer,
            pts.display(),
            duration.display(),
        );

        let state_storage = self.state.lock().unwrap();
        let state = match &*state_storage {
            Some(ref state) => state,
            None => return None,
        };

        let duration = if duration.is_some() {
            duration
        } else if let Some(ref audio_info) = state.audio_info {
            gst::ClockTime::SECOND.mul_div_floor(
                buffer.size() as u64,
                audio_info.rate() as u64 * audio_info.bpf() as u64,
            )
        } else if let Some(ref video_info) = state.video_info {
            if *video_info.fps().numer() > 0 {
                gst::ClockTime::SECOND.mul_div_floor(
                    *video_info.fps().denom() as u64,
                    *video_info.fps().numer() as u64,
                )
            } else {
                gst::ClockTime::NONE
            }
        } else {
            unreachable!()
        };

        gst_debug!(
            CAT,
            obj: agg_pad,
            "Clipping buffer {:?} with PTS {} and duration {}",
            buffer,
            pts.display(),
            duration.display(),
        );

        if agg_pad == &self.video_pad {
            let end_pts = pts
                .zip(duration)
                .and_then(|(pts, duration)| pts.checked_add(duration));

            segment.clip(pts, end_pts).map(|(start, stop)| {
                {
                    let buffer = buffer.make_mut();
                    buffer.set_pts(start);
                    buffer.set_duration(
                        stop.zip(start)
                            .and_then(|(stop, start)| stop.checked_sub(start)),
                    );
                }

                buffer
            })
        } else if let Some(ref audio_info) = state.audio_info {
            gst_audio::audio_buffer_clip(
                buffer,
                segment.upcast_ref(),
                audio_info.rate(),
                audio_info.bpf(),
            )
        } else {
            // Can't really have audio buffers without caps
            unreachable!();
        }
    }

    fn aggregate(
        &self,
        agg: &Self::Type,
        timeout: bool,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        // FIXME: Can't really happen because we always return NONE from get_next_time() but that
        // should be improved!
        assert!(!timeout);

        // Because peek_buffer() can call into clip() and that would take the state lock again,
        // first try getting buffers from both pads here
        let video_buffer_and_segment = match self.video_pad.peek_buffer() {
            Some(video_buffer) => {
                let video_segment = self.video_pad.segment();
                let video_segment = match video_segment.downcast::<gst::ClockTime>() {
                    Ok(video_segment) => video_segment,
                    Err(video_segment) => {
                        gst_error!(
                            CAT,
                            obj: agg,
                            "Video segment of wrong format {:?}",
                            video_segment.format()
                        );
                        return Err(gst::FlowError::Error);
                    }
                };

                Some((video_buffer, video_segment))
            }
            None if !self.video_pad.is_eos() => {
                gst_trace!(CAT, obj: agg, "Waiting for video buffer");
                return Err(gst_base::AGGREGATOR_FLOW_NEED_DATA);
            }
            None => None,
        };

        let audio_buffer_segment_and_pad;
        if let Some(audio_pad) = self.audio_pad.lock().unwrap().clone() {
            audio_buffer_segment_and_pad = match audio_pad.peek_buffer() {
                Some(audio_buffer) if audio_buffer.size() == 0 => {
                    // Skip empty/gap audio buffer
                    audio_pad.drop_buffer();
                    gst_trace!(CAT, obj: agg, "Empty audio buffer, waiting for next");
                    return Err(gst_base::AGGREGATOR_FLOW_NEED_DATA);
                }
                Some(audio_buffer) => {
                    let audio_segment = audio_pad.segment();
                    let audio_segment = match audio_segment.downcast::<gst::ClockTime>() {
                        Ok(audio_segment) => audio_segment,
                        Err(audio_segment) => {
                            gst_error!(
                                CAT,
                                obj: agg,
                                "Audio segment of wrong format {:?}",
                                audio_segment.format()
                            );
                            return Err(gst::FlowError::Error);
                        }
                    };

                    Some((audio_buffer, audio_segment, audio_pad))
                }
                None if !audio_pad.is_eos() => {
                    gst_trace!(CAT, obj: agg, "Waiting for audio buffer");
                    return Err(gst_base::AGGREGATOR_FLOW_NEED_DATA);
                }
                None => None,
            };
        } else {
            audio_buffer_segment_and_pad = None;
        }

        let mut state_storage = self.state.lock().unwrap();
        let state = match &mut *state_storage {
            Some(ref mut state) => state,
            None => return Err(gst::FlowError::Flushing),
        };

        let (mut current_video_buffer, current_video_running_time_end, next_video_buffer) =
            if let Some((video_buffer, video_segment)) = video_buffer_and_segment {
                let video_running_time = video_segment.to_running_time(video_buffer.pts()).unwrap();

                match state.current_video_buffer {
                    None => {
                        gst_trace!(CAT, obj: agg, "First video buffer, waiting for second");
                        state.current_video_buffer = Some((video_buffer, video_running_time));
                        drop(state_storage);
                        self.video_pad.drop_buffer();
                        return Err(gst_base::AGGREGATOR_FLOW_NEED_DATA);
                    }
                    Some((ref buffer, _)) => (
                        buffer.clone(),
                        Some(video_running_time),
                        Some((video_buffer, video_running_time)),
                    ),
                }
            } else {
                match (&state.current_video_buffer, &audio_buffer_segment_and_pad) {
                    (None, None) => {
                        gst_trace!(
                            CAT,
                            obj: agg,
                            "All pads are EOS and no buffers are queued, finishing"
                        );
                        return Err(gst::FlowError::Eos);
                    }
                    (None, Some((ref audio_buffer, ref audio_segment, _))) => {
                        // Create an empty dummy buffer for attaching the audio. This is going to
                        // be dropped by the sink later.
                        let audio_running_time =
                            audio_segment.to_running_time(audio_buffer.pts()).unwrap();

                        let video_segment = self.video_pad.segment();
                        let video_segment = match video_segment.downcast::<gst::ClockTime>() {
                            Ok(video_segment) => video_segment,
                            Err(video_segment) => {
                                gst_error!(
                                    CAT,
                                    obj: agg,
                                    "Video segment of wrong format {:?}",
                                    video_segment.format()
                                );
                                return Err(gst::FlowError::Error);
                            }
                        };
                        let video_pts =
                            video_segment.position_from_running_time(audio_running_time);
                        if video_pts.is_none() {
                            gst_warning!(CAT, obj: agg, "Can't output more audio after video EOS");
                            return Err(gst::FlowError::Eos);
                        }

                        let mut buffer = gst::Buffer::new();
                        {
                            let buffer = buffer.get_mut().unwrap();
                            buffer.set_pts(video_pts);
                        }

                        (buffer, gst::ClockTime::NONE, None)
                    }
                    (Some((ref buffer, _)), _) => (buffer.clone(), gst::ClockTime::NONE, None),
                }
            };

        if let Some((audio_buffer, audio_segment, audio_pad)) = audio_buffer_segment_and_pad {
            let audio_info = match state.audio_info {
                Some(ref audio_info) => audio_info,
                None => {
                    gst_error!(CAT, obj: agg, "Have no audio caps");
                    return Err(gst::FlowError::NotNegotiated);
                }
            };

            let audio_running_time = audio_segment.to_running_time(audio_buffer.pts());
            let duration = gst::ClockTime::SECOND.mul_div_floor(
                audio_buffer.size() as u64 / audio_info.bpf() as u64,
                audio_info.rate() as u64,
            );
            let audio_running_time_end = audio_running_time
                .zip(duration)
                .and_then(|(running_time, duration)| running_time.checked_add(duration));

            if audio_running_time_end
                .zip(current_video_running_time_end)
                .map(|(audio, video)| audio <= video)
                .unwrap_or(true)
            {
                let timecode = agg
                    .base_time()
                    .zip(audio_running_time)
                    .map(|(base_time, audio_running_time)| {
                        ((base_time.nseconds() + audio_running_time.nseconds()) / 100) as i64
                    })
                    .unwrap_or(crate::ndisys::NDIlib_send_timecode_synthesize);

                gst_trace!(
                    CAT,
                    obj: agg,
                    "Including audio buffer {:?} with timecode {}: {} <= {}",
                    audio_buffer,
                    timecode,
                    audio_running_time_end.display(),
                    current_video_running_time_end.display(),
                );
                state
                    .current_audio_buffers
                    .push((audio_buffer, audio_info.clone(), timecode));
                audio_pad.drop_buffer();

                // If there is still video data, wait for the next audio buffer or EOS,
                // otherwise just output the dummy video buffer directly.
                if current_video_running_time_end.is_some() {
                    return Err(gst_base::AGGREGATOR_FLOW_NEED_DATA);
                }
            }

            // Otherwise finish this video buffer with all audio that has accumulated so
            // far
        }

        let audio_buffers = mem::take(&mut state.current_audio_buffers);

        if !audio_buffers.is_empty() {
            let current_video_buffer = current_video_buffer.make_mut();
            crate::ndisinkmeta::NdiSinkAudioMeta::add(current_video_buffer, audio_buffers);
        }

        if let Some((video_buffer, video_running_time)) = next_video_buffer {
            state.current_video_buffer = Some((video_buffer, video_running_time));
            drop(state_storage);
            self.video_pad.drop_buffer();
        } else {
            state.current_video_buffer = None;
            drop(state_storage);
        }

        gst_trace!(
            CAT,
            obj: agg,
            "Finishing video buffer {:?}",
            current_video_buffer
        );
        agg.finish_buffer(current_video_buffer)
    }

    fn sink_event(
        &self,
        agg: &Self::Type,
        pad: &gst_base::AggregatorPad,
        event: gst::Event,
    ) -> bool {
        use gst::EventView;

        match event.view() {
            EventView::Caps(caps) => {
                let caps = caps.caps_owned();

                let mut state_storage = self.state.lock().unwrap();
                let state = match &mut *state_storage {
                    Some(ref mut state) => state,
                    None => return false,
                };

                if pad == &self.video_pad {
                    let info = match gst_video::VideoInfo::from_caps(&caps) {
                        Ok(info) => info,
                        Err(_) => {
                            gst_error!(CAT, obj: pad, "Failed to parse caps {:?}", caps);
                            return false;
                        }
                    };

                    // 2 frames latency because we queue 1 frame and wait until audio
                    // up to the end of that frame has arrived.
                    let latency = if *info.fps().numer() > 0 {
                        gst::ClockTime::SECOND
                            .mul_div_floor(
                                2 * *info.fps().denom() as u64,
                                *info.fps().numer() as u64,
                            )
                            .unwrap_or(80 * gst::ClockTime::MSECOND)
                    } else {
                        // let's assume 25fps and 2 frames latency
                        80 * gst::ClockTime::MSECOND
                    };

                    state.video_info = Some(info);

                    drop(state_storage);

                    agg.set_latency(latency, gst::ClockTime::NONE);

                    // The video caps are passed through as the audio is included only in a meta
                    agg.set_src_caps(&caps);
                } else {
                    let info = match gst_audio::AudioInfo::from_caps(&caps) {
                        Ok(info) => info,
                        Err(_) => {
                            gst_error!(CAT, obj: pad, "Failed to parse caps {:?}", caps);
                            return false;
                        }
                    };

                    state.audio_info = Some(info);
                }
            }
            // The video segment is passed through as-is and the video timestamps are preserved
            EventView::Segment(segment) if pad == &self.video_pad => {
                let segment = segment.segment();
                gst_debug!(CAT, obj: agg, "Updating segment {:?}", segment);
                agg.update_segment(segment);
            }
            _ => (),
        }

        self.parent_sink_event(agg, pad, event)
    }

    fn sink_query(
        &self,
        agg: &Self::Type,
        pad: &gst_base::AggregatorPad,
        query: &mut gst::QueryRef,
    ) -> bool {
        use gst::QueryView;

        match query.view_mut() {
            QueryView::Caps(_) if pad == &self.video_pad => {
                // Directly forward caps queries
                let srcpad = agg.static_pad("src").unwrap();
                return srcpad.peer_query(query);
            }
            _ => (),
        }

        self.parent_sink_query(agg, pad, query)
    }

    fn negotiate(&self, _agg: &Self::Type) -> bool {
        // No negotiation needed as the video caps are just passed through
        true
    }
}
