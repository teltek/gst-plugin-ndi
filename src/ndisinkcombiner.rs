use glib::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::{gst_debug, gst_error, gst_trace, gst_warning};
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;

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

struct NdiSinkCombiner {
    video_pad: gst_base::AggregatorPad,
    audio_pad: Mutex<Option<gst_base::AggregatorPad>>,
    state: Mutex<Option<State>>,
}

impl ObjectSubclass for NdiSinkCombiner {
    const NAME: &'static str = "NdiSinkCombiner";
    type ParentType = gst_base::Aggregator;
    type Instance = gst::subclass::ElementInstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib::glib_object_subclass!();

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.set_metadata(
            "NDI Sink Combiner",
            "Combiner/Audio/Video",
            "NDI sink audio/video combiner",
            "Sebastian Dröge <sebastian@centricular.com>",
        );

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
        klass.add_pad_template(src_pad_template);

        let sink_pad_template = gst::PadTemplate::with_gtype(
            "video",
            gst::PadDirection::Sink,
            gst::PadPresence::Always,
            &caps,
            gst_base::AggregatorPad::static_type(),
        )
        .unwrap();
        klass.add_pad_template(sink_pad_template);

        let caps = gst::Caps::builder("audio/x-raw")
            .field("format", &gst_audio::AUDIO_FORMAT_S16.to_str())
            .field("rate", &gst::IntRange::<i32>::new(1, i32::MAX))
            .field("channels", &gst::IntRange::<i32>::new(1, i32::MAX))
            .field("layout", &"interleaved")
            .build();
        let sink_pad_template = gst::PadTemplate::with_gtype(
            "audio",
            gst::PadDirection::Sink,
            gst::PadPresence::Request,
            &caps,
            gst_base::AggregatorPad::static_type(),
        )
        .unwrap();
        klass.add_pad_template(sink_pad_template);
    }

    fn with_class(klass: &Self::Class) -> Self {
        let templ = klass.get_pad_template("video").unwrap();
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
    glib::glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let element = obj.downcast_ref::<gst::Element>().unwrap();
        element.add_pad(&self.video_pad).unwrap();

        self.parent_constructed(obj);
    }
}

impl ElementImpl for NdiSinkCombiner {
    fn release_pad(&self, element: &gst::Element, pad: &gst::Pad) {
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
        agg: &gst_base::Aggregator,
        templ: &gst::PadTemplate,
        _req_name: Option<&str>,
        _caps: Option<&gst::Caps>,
    ) -> Option<gst_base::AggregatorPad> {
        let mut audio_pad_storage = self.audio_pad.lock().unwrap();

        if audio_pad_storage.is_some() {
            gst_error!(CAT, obj: agg, "Audio pad already requested");
            return None;
        }

        let sink_templ = agg.get_pad_template("audio").unwrap();
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

    fn start(&self, agg: &gst_base::Aggregator) -> Result<(), gst::ErrorMessage> {
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

    fn stop(&self, agg: &gst_base::Aggregator) -> Result<(), gst::ErrorMessage> {
        // Drop our state now
        let _ = self.state.lock().unwrap().take();

        gst_debug!(CAT, obj: agg, "Stopped");

        Ok(())
    }

    fn get_next_time(&self, _agg: &gst_base::Aggregator) -> gst::ClockTime {
        // FIXME: What to do here? We don't really know when the next buffer is expected
        gst::CLOCK_TIME_NONE
    }

    fn clip(
        &self,
        agg: &gst_base::Aggregator,
        agg_pad: &gst_base::AggregatorPad,
        mut buffer: gst::Buffer,
    ) -> Option<gst::Buffer> {
        let segment = match agg_pad.get_segment().downcast::<gst::ClockTime>() {
            Ok(segment) => segment,
            Err(_) => {
                gst_error!(CAT, obj: agg, "Only TIME segments supported");
                return Some(buffer);
            }
        };

        let pts = buffer.get_pts();
        if pts.is_none() {
            gst_error!(CAT, obj: agg, "Only buffers with PTS supported");
            return Some(buffer);
        }

        let duration = if buffer.get_duration().is_some() {
            buffer.get_duration()
        } else {
            gst::CLOCK_TIME_NONE
        };

        gst_trace!(
            CAT,
            obj: agg_pad,
            "Clipping buffer {:?} with PTS {} and duration {}",
            buffer,
            pts,
            duration
        );

        let state_storage = self.state.lock().unwrap();
        let state = match &*state_storage {
            Some(ref state) => state,
            None => return None,
        };

        let duration = if buffer.get_duration().is_some() {
            buffer.get_duration()
        } else if let Some(ref audio_info) = state.audio_info {
            gst::SECOND
                .mul_div_floor(
                    buffer.get_size() as u64,
                    audio_info.rate() as u64 * audio_info.bpf() as u64,
                )
                .unwrap()
        } else if let Some(ref video_info) = state.video_info {
            if *video_info.fps().numer() > 0 {
                gst::SECOND
                    .mul_div_floor(
                        *video_info.fps().denom() as u64,
                        *video_info.fps().numer() as u64,
                    )
                    .unwrap()
            } else {
                gst::CLOCK_TIME_NONE
            }
        } else {
            unreachable!()
        };

        gst_debug!(
            CAT,
            obj: agg_pad,
            "Clipping buffer {:?} with PTS {} and duration {}",
            buffer,
            pts,
            duration
        );

        if agg_pad == &self.video_pad {
            segment.clip(pts, pts + duration).map(|(start, stop)| {
                {
                    let buffer = buffer.make_mut();
                    buffer.set_pts(start);
                    if duration.is_some() {
                        buffer.set_duration(stop - start);
                    }
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
        agg: &gst_base::Aggregator,
        timeout: bool,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        // FIXME: Can't really happen because we always return NONE from get_next_time() but that
        // should be improved!
        assert!(!timeout);

        // Because peek_buffer() can call into clip() and that would take the state lock again,
        // first try getting buffers from both pads here
        let video_buffer_and_segment = match self.video_pad.peek_buffer() {
            Some(video_buffer) => {
                let video_segment = self.video_pad.get_segment();
                let video_segment = match video_segment.downcast::<gst::ClockTime>() {
                    Ok(video_segment) => video_segment,
                    Err(video_segment) => {
                        gst_error!(
                            CAT,
                            obj: agg,
                            "Video segment of wrong format {:?}",
                            video_segment.get_format()
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
                Some(audio_buffer) if audio_buffer.get_size() == 0 => {
                    // Skip empty/gap audio buffer
                    audio_pad.drop_buffer();
                    gst_trace!(CAT, obj: agg, "Empty audio buffer, waiting for next");
                    return Err(gst_base::AGGREGATOR_FLOW_NEED_DATA);
                }
                Some(audio_buffer) => {
                    let audio_segment = audio_pad.get_segment();
                    let audio_segment = match audio_segment.downcast::<gst::ClockTime>() {
                        Ok(audio_segment) => audio_segment,
                        Err(audio_segment) => {
                            gst_error!(
                                CAT,
                                obj: agg,
                                "Audio segment of wrong format {:?}",
                                audio_segment.get_format()
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
                let video_running_time = video_segment.to_running_time(video_buffer.get_pts());
                assert!(video_running_time.is_some());

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
                        video_running_time,
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
                            audio_segment.to_running_time(audio_buffer.get_pts());
                        assert!(audio_running_time.is_some());

                        let video_segment = self.video_pad.get_segment();
                        let video_segment = match video_segment.downcast::<gst::ClockTime>() {
                            Ok(video_segment) => video_segment,
                            Err(video_segment) => {
                                gst_error!(
                                    CAT,
                                    obj: agg,
                                    "Video segment of wrong format {:?}",
                                    video_segment.get_format()
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

                        (buffer, gst::CLOCK_TIME_NONE, None)
                    }
                    (Some((ref buffer, _)), _) => (buffer.clone(), gst::CLOCK_TIME_NONE, None),
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

            let audio_running_time = audio_segment.to_running_time(audio_buffer.get_pts());
            assert!(audio_running_time.is_some());
            let duration = gst::SECOND
                .mul_div_floor(
                    audio_buffer.get_size() as u64 / audio_info.bpf() as u64,
                    audio_info.rate() as u64,
                )
                .unwrap_or(gst::CLOCK_TIME_NONE);
            let audio_running_time_end = audio_running_time + duration;
            assert!(audio_running_time_end.is_some());

            if audio_running_time_end <= current_video_running_time_end
                || current_video_running_time_end.is_none()
            {
                let timecode = (audio_running_time + agg.get_base_time())
                    .map(|t| (t / 100) as i64)
                    .unwrap_or(crate::ndisys::NDIlib_send_timecode_synthesize);

                gst_trace!(
                    CAT,
                    obj: agg,
                    "Including audio buffer {:?} with timecode {}: {} <= {}",
                    audio_buffer,
                    timecode,
                    audio_running_time_end,
                    current_video_running_time_end,
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

        let audio_buffers = mem::replace(&mut state.current_audio_buffers, Vec::new());

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
        agg: &gst_base::Aggregator,
        pad: &gst_base::AggregatorPad,
        event: gst::Event,
    ) -> bool {
        use gst::EventView;

        match event.view() {
            EventView::Caps(caps) => {
                let caps = caps.get_caps_owned();

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
                        gst::SECOND
                            .mul_div_floor(
                                2 * *info.fps().denom() as u64,
                                *info.fps().numer() as u64,
                            )
                            .unwrap_or(80 * gst::MSECOND)
                    } else {
                        // let's assume 25fps and 2 frames latency
                        80 * gst::MSECOND
                    };

                    state.video_info = Some(info);

                    drop(state_storage);

                    agg.set_latency(latency, gst::CLOCK_TIME_NONE);

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
                let segment = segment.get_segment();
                gst_debug!(CAT, obj: agg, "Updating segment {:?}", segment);
                agg.update_segment(segment);
            }
            _ => (),
        }

        self.parent_sink_event(agg, pad, event)
    }

    fn sink_query(
        &self,
        agg: &gst_base::Aggregator,
        pad: &gst_base::AggregatorPad,
        query: &mut gst::QueryRef,
    ) -> bool {
        use gst::QueryView;

        match query.view_mut() {
            QueryView::Caps(_) if pad == &self.video_pad => {
                // Directly forward caps queries
                let srcpad = agg.get_static_pad("src").unwrap();
                return srcpad.peer_query(query);
            }
            _ => (),
        }

        self.parent_sink_query(agg, pad, query)
    }

    fn negotiate(&self, _agg: &gst_base::Aggregator) -> bool {
        // No negotiation needed as the video caps are just passed through
        true
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndisinkcombiner",
        gst::Rank::None,
        NdiSinkCombiner::get_type(),
    )
}
