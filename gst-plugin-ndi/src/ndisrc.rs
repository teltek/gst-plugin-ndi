// Copyright (C) 2018 Sebastian Dröge <sebastian@centricular.com>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use glib;
use gst;
use gst::prelude::*;
use gst_audio;
use gst_base::prelude::*;

use byte_slice_cast::*;

use gst_plugin::base_src::*;
use gst_plugin::element::*;
use gst_plugin::object::*;
use gst_plugin::properties::*;

use std::ops::Rem;
use std::sync::Mutex;
use std::{i32, u32};

use num_traits::cast::NumCast;
use num_traits::float::Float;

// Default values of properties
const DEFAULT_SAMPLES_PER_BUFFER: u32 = 1024;
const DEFAULT_FREQ: u32 = 440;
const DEFAULT_VOLUME: f64 = 0.8;
const DEFAULT_MUTE: bool = false;
const DEFAULT_IS_LIVE: bool = false;

// Property value storage
#[derive(Debug, Clone)]
struct Settings {
    stream_name: String,
    samples_per_buffer: u32,
    freq: u32,
    volume: f64,
    mute: bool,
    is_live: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            stream_name: String::from("Fixed ndi stream name"),
            samples_per_buffer: DEFAULT_SAMPLES_PER_BUFFER,
            freq: DEFAULT_FREQ,
            volume: DEFAULT_VOLUME,
            mute: DEFAULT_MUTE,
            is_live: DEFAULT_IS_LIVE,
        }
    }
}

// Metadata for the properties
static PROPERTIES: [Property; 6] = [
    Property::String(
        "stream-name",
        "Sream Name",
        "Name of the streaming device",
        None,
        PropertyMutability::ReadWrite,
    ),
    Property::UInt(
        "samples-per-buffer",
        "Samples Per Buffer",
        "Number of samples per output buffer",
        (1, u32::MAX),
        DEFAULT_SAMPLES_PER_BUFFER,
        PropertyMutability::ReadWrite,
    ),
    Property::UInt(
        "freq",
        "Frequency",
        "Frequency",
        (1, u32::MAX),
        DEFAULT_FREQ,
        PropertyMutability::ReadWrite,
    ),
    Property::Double(
        "volume",
        "Volume",
        "Output volume",
        (0.0, 10.0),
        DEFAULT_VOLUME,
        PropertyMutability::ReadWrite,
    ),
    Property::Boolean(
        "mute",
        "Mute",
        "Mute",
        DEFAULT_MUTE,
        PropertyMutability::ReadWrite,
    ),
    Property::Boolean(
        "is-live",
        "Is Live",
        "(Pseudo) live output",
        DEFAULT_IS_LIVE,
        PropertyMutability::ReadWrite,
    ),
];

// Stream-specific state, i.e. audio format configuration
// and sample offset
struct State {
    info: Option<gst_audio::AudioInfo>,
    sample_offset: u64,
    sample_stop: Option<u64>,
    accumulator: f64,
}

impl Default for State {
    fn default() -> State {
        State {
            info: None,
            sample_offset: 0,
            sample_stop: None,
            accumulator: 0.0,
        }
    }
}

struct ClockWait {
    clock_id: Option<gst::ClockId>,
    flushing: bool,
}

// Struct containing all the element data
struct NdiSrc {
    cat: gst::DebugCategory,
    settings: Mutex<Settings>,
    state: Mutex<State>,
    clock_wait: Mutex<ClockWait>,
}

impl NdiSrc {
    // Called when a new instance is to be created
    fn new(element: &BaseSrc) -> Box<BaseSrcImpl<BaseSrc>> {
        // Initialize live-ness and notify the base class that
        // we'd like to operate in Time format
        element.set_live(DEFAULT_IS_LIVE);
        element.set_format(gst::Format::Time);

        Box::new(Self {
            cat: gst::DebugCategory::new(
                "ndisrc",
                gst::DebugColorFlags::empty(),
                "NewTek NDI Source",
            ),
            settings: Mutex::new(Default::default()),
            state: Mutex::new(Default::default()),
            clock_wait: Mutex::new(ClockWait {
                clock_id: None,
                flushing: true,
            }),
        })
    }

    // Called exactly once when registering the type. Used for
    // setting up metadata for all instances, e.g. the name and
    // classification and the pad templates with their caps.
    //
    // Actual instances can create pads based on those pad templates
    // with a subset of the caps given here. In case of basesrc,
    // a "src" and "sink" pad template are required here and the base class
    // will automatically instantiate pads for them.
    //
    // Our element here can output f32 and f64
    fn class_init(klass: &mut BaseSrcClass) {
        klass.set_metadata(
            "NewTek NDI Source",
            "Source",
            "NewTek NDI video/audio source",
            "Ruben Gonzalez <rubenrua@teltek.es>",
        );

        // On the src pad, we can produce F32/F64 with any sample rate
        // and any number of channels
        let caps = gst::Caps::new_simple(
            "audio/x-raw",
            &[
                (
                    "format",
                    &gst::List::new(&[
                        &gst_audio::AUDIO_FORMAT_F32.to_string(),
                        &gst_audio::AUDIO_FORMAT_F64.to_string(),
                    ]),
                ),
                ("layout", &"interleaved"),
                ("rate", &gst::IntRange::<i32>::new(1, i32::MAX)),
                ("channels", &gst::IntRange::<i32>::new(1, i32::MAX)),
            ],
        );
        // The src pad template must be named "src" for basesrc
        // and specific a pad that is always there
        let src_pad_template = gst::PadTemplate::new(
            "src",
            gst::PadDirection::Src,
            gst::PadPresence::Always,
            &caps,
        );
        klass.add_pad_template(src_pad_template);

        // Install all our properties
        klass.install_properties(&PROPERTIES);
    }

    fn process<F: Float + FromByteSlice>(
        data: &mut [u8],
        accumulator_ref: &mut f64,
        freq: u32,
        rate: u32,
        channels: u32,
        vol: f64,
    ) {
        use std::f64::consts::PI;

        // Reinterpret our byte-slice as a slice containing elements of the type
        // we're interested in. GStreamer requires for raw audio that the alignment
        // of memory is correct, so this will never ever fail unless there is an
        // actual bug elsewhere.
        let data = data.as_mut_slice_of::<F>().unwrap();

        // Convert all our parameters to the target type for calculations
        let vol: F = NumCast::from(vol).unwrap();
        let freq = freq as f64;
        let rate = rate as f64;
        let two_pi = 2.0 * PI;

        // We're carrying a accumulator with up to 2pi around instead of working
        // on the sample offset. High sample offsets cause too much inaccuracy when
        // converted to floating point numbers and then iterated over in 1-steps
        let mut accumulator = *accumulator_ref;
        let step = two_pi * freq / rate;

        for chunk in data.chunks_mut(channels as usize) {
            let value = vol * F::sin(NumCast::from(accumulator).unwrap());
            for sample in chunk {
                *sample = value;
            }

            accumulator += step;
            if accumulator >= two_pi {
                accumulator -= two_pi;
            }
        }

        *accumulator_ref = accumulator;
    }
}

// Virtual methods of GObject itself
impl ObjectImpl<BaseSrc> for NdiSrc {
    // Called whenever a value of a property is changed. It can be called
    // at any time from any thread.
    fn set_property(&self, obj: &glib::Object, id: u32, value: &glib::Value) {
        let prop = &PROPERTIES[id as usize];
        let element = obj.clone().downcast::<BaseSrc>().unwrap();

        match *prop {
            Property::String("stream-name", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let stream_name = value.get().unwrap();
                gst_info!(
                    self.cat,
                    obj: &element,
                    "Changing stream-name from {} to {}",
                    settings.stream_name,
                    stream_name
                );
                settings.stream_name = stream_name;
                drop(settings);

                let _ =
                    element.post_message(&gst::Message::new_latency().src(Some(&element)).build());
            }
            Property::UInt("samples-per-buffer", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let samples_per_buffer = value.get().unwrap();
                gst_info!(
                    self.cat,
                    obj: &element,
                    "Changing samples-per-buffer from {} to {}",
                    settings.samples_per_buffer,
                    samples_per_buffer
                );
                settings.samples_per_buffer = samples_per_buffer;
                drop(settings);

                let _ =
                    element.post_message(&gst::Message::new_latency().src(Some(&element)).build());
            }
            Property::UInt("freq", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let freq = value.get().unwrap();
                gst_info!(
                    self.cat,
                    obj: &element,
                    "Changing freq from {} to {}",
                    settings.freq,
                    freq
                );
                settings.freq = freq;
            }
            Property::Double("volume", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let volume = value.get().unwrap();
                gst_info!(
                    self.cat,
                    obj: &element,
                    "Changing volume from {} to {}",
                    settings.volume,
                    volume
                );
                settings.volume = volume;
            }
            Property::Boolean("mute", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let mute = value.get().unwrap();
                gst_info!(
                    self.cat,
                    obj: &element,
                    "Changing mute from {} to {}",
                    settings.mute,
                    mute
                );
                settings.mute = mute;
            }
            Property::Boolean("is-live", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let is_live = value.get().unwrap();
                gst_info!(
                    self.cat,
                    obj: &element,
                    "Changing is-live from {} to {}",
                    settings.is_live,
                    is_live
                );
                settings.is_live = is_live;
            }
            _ => unimplemented!(),
        }
    }

    // Called whenever a value of a property is read. It can be called
    // at any time from any thread.
    fn get_property(&self, _obj: &glib::Object, id: u32) -> Result<glib::Value, ()> {
        let prop = &PROPERTIES[id as usize];

        match *prop {
            Property::UInt("stream-name", ..) => {
                let settings = self.settings.lock().unwrap();
                //TODO to_value supongo que solo funciona con numeros
                Ok(settings.stream_name.to_value())
            }
            Property::UInt("samples-per-buffer", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.samples_per_buffer.to_value())
            }
            Property::UInt("freq", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.freq.to_value())
            }
            Property::Double("volume", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.volume.to_value())
            }
            Property::Boolean("mute", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.mute.to_value())
            }
            Property::Boolean("is-live", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.is_live.to_value())
            }
            _ => unimplemented!(),
        }
    }
}

// Virtual methods of gst::Element. We override none
impl ElementImpl<BaseSrc> for NdiSrc {
    fn change_state(
        &self,
        element: &BaseSrc,
        transition: gst::StateChange,
    ) -> gst::StateChangeReturn {
        // Configure live'ness once here just before starting the source
        match transition {
            gst::StateChange::ReadyToPaused => {
                element.set_live(self.settings.lock().unwrap().is_live);
            }
            _ => (),
        }

        element.parent_change_state(transition)
    }
}

// Virtual methods of gst_base::BaseSrc
impl BaseSrcImpl<BaseSrc> for NdiSrc {
    // Called whenever the input/output caps are changing, i.e. in the very beginning before data
    // flow happens and whenever the situation in the pipeline is changing. All buffers after this
    // call have the caps given here.
    //
    // We simply remember the resulting AudioInfo from the caps to be able to use this for knowing
    // the sample rate, etc. when creating buffers
    fn set_caps(&self, element: &BaseSrc, caps: &gst::CapsRef) -> bool {
        use std::f64::consts::PI;

        let info = match gst_audio::AudioInfo::from_caps(caps) {
            None => return false,
            Some(info) => info,
        };

        gst_debug!(self.cat, obj: element, "Configuring for caps {}", caps);

        element.set_blocksize(info.bpf() * (*self.settings.lock().unwrap()).samples_per_buffer);

        let settings = &*self.settings.lock().unwrap();
        let mut state = self.state.lock().unwrap();

        // If we have no caps yet, any old sample_offset and sample_stop will be
        // in nanoseconds
        let old_rate = match state.info {
            Some(ref info) => info.rate() as u64,
            None => gst::SECOND_VAL,
        };

        // Update sample offset and accumulator based on the previous values and the
        // sample rate change, if any
        let old_sample_offset = state.sample_offset;
        let sample_offset = old_sample_offset
            .mul_div_floor(info.rate() as u64, old_rate)
            .unwrap();

        let old_sample_stop = state.sample_stop;
        let sample_stop =
            old_sample_stop.map(|v| v.mul_div_floor(info.rate() as u64, old_rate).unwrap());

        let accumulator =
            (sample_offset as f64).rem(2.0 * PI * (settings.freq as f64) / (info.rate() as f64));

        *state = State {
            info: Some(info),
            sample_offset: sample_offset,
            sample_stop: sample_stop,
            accumulator: accumulator,
        };

        drop(state);

        let _ = element.post_message(&gst::Message::new_latency().src(Some(element)).build());

        true
    }

    // Called when starting, so we can initialize all stream-related state to its defaults
    fn start(&self, element: &BaseSrc) -> bool {
        // Reset state
        *self.state.lock().unwrap() = Default::default();
        self.unlock_stop(element);

        gst_info!(self.cat, obj: element, "Started");

        true
    }

    // Called when shutting down the element so we can release all stream-related state
    fn stop(&self, element: &BaseSrc) -> bool {
        // Reset state
        *self.state.lock().unwrap() = Default::default();
        self.unlock(element);

        gst_info!(self.cat, obj: element, "Stopped");

        true
    }

    fn query(&self, element: &BaseSrc, query: &mut gst::QueryRef) -> bool {
        use gst::QueryView;

        match query.view_mut() {
            // We only work in Push mode. In Pull mode, create() could be called with
            // arbitrary offsets and we would have to produce for that specific offset
            QueryView::Scheduling(ref mut q) => {
                q.set(gst::SchedulingFlags::SEQUENTIAL, 1, -1, 0);
                q.add_scheduling_modes(&[gst::PadMode::Push]);
                return true;
            }
            // In Live mode we will have a latency equal to the number of samples in each buffer.
            // We can't output samples before they were produced, and the last sample of a buffer
            // is produced that much after the beginning, leading to this latency calculation
            QueryView::Latency(ref mut q) => {
                let settings = &*self.settings.lock().unwrap();
                let state = self.state.lock().unwrap();

                if let Some(ref info) = state.info {
                    let latency = gst::SECOND
                        .mul_div_floor(settings.samples_per_buffer as u64, info.rate() as u64)
                        .unwrap();
                    gst_debug!(self.cat, obj: element, "Returning latency {}", latency);
                    q.set(settings.is_live, latency, gst::CLOCK_TIME_NONE);
                    return true;
                } else {
                    return false;
                }
            }
            _ => (),
        }
        BaseSrcBase::parent_query(element, query)
    }

    // Creates the audio buffers
    fn create(
        &self,
        element: &BaseSrc,
        _offset: u64,
        _length: u32,
    ) -> Result<gst::Buffer, gst::FlowReturn> {
        // Keep a local copy of the values of all our properties at this very moment. This
        // ensures that the mutex is never locked for long and the application wouldn't
        // have to block until this function returns when getting/setting property values
        let settings = &*self.settings.lock().unwrap();

        // Get a locked reference to our state, i.e. the input and output AudioInfo
        let mut state = self.state.lock().unwrap();
        let info = match state.info {
            None => {
                gst_element_error!(element, gst::CoreError::Negotiation, ["Have no caps yet"]);
                return Err(gst::FlowReturn::NotNegotiated);
            }
            Some(ref info) => info.clone(),
        };

        // If a stop position is set (from a seek), only produce samples up to that
        // point but at most samples_per_buffer samples per buffer
        let n_samples = if let Some(sample_stop) = state.sample_stop {
            if sample_stop <= state.sample_offset {
                gst_log!(self.cat, obj: element, "At EOS");
                return Err(gst::FlowReturn::Eos);
            }

            sample_stop - state.sample_offset
        } else {
            settings.samples_per_buffer as u64
        };

        // Allocate a new buffer of the required size, update the metadata with the
        // current timestamp and duration and then fill it according to the current
        // caps
        let mut buffer =
            gst::Buffer::with_size((n_samples as usize) * (info.bpf() as usize)).unwrap();
        {
            let buffer = buffer.get_mut().unwrap();

            // Calculate the current timestamp (PTS) and the next one,
            // and calculate the duration from the difference instead of
            // simply the number of samples to prevent rounding errors
            let pts = state
                .sample_offset
                .mul_div_floor(gst::SECOND_VAL, info.rate() as u64)
                .unwrap()
                .into();
            let next_pts: gst::ClockTime = (state.sample_offset + n_samples)
                .mul_div_floor(gst::SECOND_VAL, info.rate() as u64)
                .unwrap()
                .into();
            buffer.set_pts(pts);
            buffer.set_duration(next_pts - pts);

            // Map the buffer writable and create the actual samples
            let mut map = buffer.map_writable().unwrap();
            let data = map.as_mut_slice();

            if info.format() == gst_audio::AUDIO_FORMAT_F32 {
                Self::process::<f32>(
                    data,
                    &mut state.accumulator,
                    settings.freq,
                    info.rate(),
                    info.channels(),
                    settings.volume,
                );
            } else {
                Self::process::<f64>(
                    data,
                    &mut state.accumulator,
                    settings.freq,
                    info.rate(),
                    info.channels(),
                    settings.volume,
                );
            }
        }
        state.sample_offset += n_samples;
        drop(state);

        // If we're live, we are waiting until the time of the last sample in our buffer has
        // arrived. This is the very reason why we have to report that much latency.
        // A real live-source would of course only allow us to have the data available after
        // that latency, e.g. when capturing from a microphone, and no waiting from our side
        // would be necessary..
        //
        // Waiting happens based on the pipeline clock, which means that a real live source
        // with its own clock would require various translations between the two clocks.
        // This is out of scope for the tutorial though.
        if element.is_live() {
            let clock = match element.get_clock() {
                None => return Ok(buffer),
                Some(clock) => clock,
            };

            let segment = element
                .get_segment()
                .downcast::<gst::format::Time>()
                .unwrap();
            let base_time = element.get_base_time();
            let running_time = segment.to_running_time(buffer.get_pts() + buffer.get_duration());

            // The last sample's clock time is the base time of the element plus the
            // running time of the last sample
            let wait_until = running_time + base_time;
            if wait_until.is_none() {
                return Ok(buffer);
            }

            // Store the clock ID in our struct unless we're flushing anyway.
            // This allows to asynchronously cancel the waiting from unlock()
            // so that we immediately stop waiting on e.g. shutdown.
            let mut clock_wait = self.clock_wait.lock().unwrap();
            if clock_wait.flushing {
                gst_debug!(self.cat, obj: element, "Flushing");
                return Err(gst::FlowReturn::Flushing);
            }

            let id = clock.new_single_shot_id(wait_until).unwrap();
            clock_wait.clock_id = Some(id.clone());
            drop(clock_wait);

            gst_log!(
                self.cat,
                obj: element,
                "Waiting until {}, now {}",
                wait_until,
                clock.get_time()
            );
            let (res, jitter) = id.wait();
            gst_log!(
                self.cat,
                obj: element,
                "Waited res {:?} jitter {}",
                res,
                jitter
            );
            self.clock_wait.lock().unwrap().clock_id.take();

            // If the clock ID was unscheduled, unlock() was called
            // and we should return Flushing immediately.
            if res == gst::ClockReturn::Unscheduled {
                gst_debug!(self.cat, obj: element, "Flushing");
                return Err(gst::FlowReturn::Flushing);
            }
        }

        gst_debug!(self.cat, obj: element, "Produced buffer {:?}", buffer);

        Ok(buffer)
    }

    fn fixate(&self, element: &BaseSrc, caps: gst::Caps) -> gst::Caps {
        // Fixate the caps. BaseSrc will do some fixation for us, but
        // as we allow any rate between 1 and MAX it would fixate to 1. 1Hz
        // is generally not a useful sample rate.
        //
        // We fixate to the closest integer value to 48kHz that is possible
        // here, and for good measure also decide that the closest value to 1
        // channel is good.
        let mut caps = gst::Caps::truncate(caps);
        {
            let caps = caps.make_mut();
            let s = caps.get_mut_structure(0).unwrap();
            s.fixate_field_nearest_int("rate", 48_000);
            s.fixate_field_nearest_int("channels", 1);
        }

        // Let BaseSrc fixate anything else for us. We could've alternatively have
        // called Caps::fixate() here
        element.parent_fixate(caps)
    }

    fn is_seekable(&self, _element: &BaseSrc) -> bool {
        false
    }

    // fn do_seek(&self, element: &BaseSrc, segment: &mut gst::Segment) -> bool {
    //     // Handle seeking here. For Time and Default (sample offset) seeks we can
    //     // do something and have to update our sample offset and accumulator accordingly.
    //     //
    //     // Also we should remember the stop time (so we can stop at that point), and if
    //     // reverse playback is requested. These values will all be used during buffer creation
    //     // and for calculating the timestamps, etc.
    //
    //     if segment.get_rate() < 0.0 {
    //         gst_error!(self.cat, obj: element, "Reverse playback not supported");
    //         return false;
    //     }
    //
    //     let settings = *self.settings.lock().unwrap();
    //     let mut state = self.state.lock().unwrap();
    //
    //     // We store sample_offset and sample_stop in nanoseconds if we
    //     // don't know any sample rate yet. It will be converted correctly
    //     // once a sample rate is known.
    //     let rate = match state.info {
    //         None => gst::SECOND_VAL,
    //         Some(ref info) => info.rate() as u64,
    //     };
    //
    //     if let Some(segment) = segment.downcast_ref::<gst::format::Time>() {
    //         use std::f64::consts::PI;
    //
    //         let sample_offset = segment
    //             .get_start()
    //             .unwrap()
    //             .mul_div_floor(rate, gst::SECOND_VAL)
    //             .unwrap();
    //
    //         let sample_stop = segment
    //             .get_stop()
    //             .map(|v| v.mul_div_floor(rate, gst::SECOND_VAL).unwrap());
    //
    //         let accumulator =
    //             (sample_offset as f64).rem(2.0 * PI * (settings.freq as f64) / (rate as f64));
    //
    //         gst_debug!(
    //             self.cat,
    //             obj: element,
    //             "Seeked to {}-{:?} (accum: {}) for segment {:?}",
    //             sample_offset,
    //             sample_stop,
    //             accumulator,
    //             segment
    //         );
    //
    //         *state = State {
    //             info: state.info.clone(),
    //             sample_offset: sample_offset,
    //             sample_stop: sample_stop,
    //             accumulator: accumulator,
    //         };
    //
    //         true
    //     } else if let Some(segment) = segment.downcast_ref::<gst::format::Default>() {
    //         use std::f64::consts::PI;
    //
    //         if state.info.is_none() {
    //             gst_error!(
    //                 self.cat,
    //                 obj: element,
    //                 "Can only seek in Default format if sample rate is known"
    //             );
    //             return false;
    //         }
    //
    //         let sample_offset = segment.get_start().unwrap();
    //         let sample_stop = segment.get_stop().0;
    //
    //         let accumulator =
    //             (sample_offset as f64).rem(2.0 * PI * (settings.freq as f64) / (rate as f64));
    //
    //         gst_debug!(
    //             self.cat,
    //             obj: element,
    //             "Seeked to {}-{:?} (accum: {}) for segment {:?}",
    //             sample_offset,
    //             sample_stop,
    //             accumulator,
    //             segment
    //         );
    //
    //         *state = State {
    //             info: state.info.clone(),
    //             sample_offset: sample_offset,
    //             sample_stop: sample_stop,
    //             accumulator: accumulator,
    //         };
    //
    //         true
    //     } else {
    //         gst_error!(
    //             self.cat,
    //             obj: element,
    //             "Can't seek in format {:?}",
    //             segment.get_format()
    //         );
    //
    //         false
    //     }
    // }

    fn unlock(&self, element: &BaseSrc) -> bool {
        // This should unblock the create() function ASAP, so we
        // just unschedule the clock it here, if any.
        gst_debug!(self.cat, obj: element, "Unlocking");
        let mut clock_wait = self.clock_wait.lock().unwrap();
        if let Some(clock_id) = clock_wait.clock_id.take() {
            clock_id.unschedule();
        }
        clock_wait.flushing = true;

        true
    }

    fn unlock_stop(&self, element: &BaseSrc) -> bool {
        // This signals that unlocking is done, so we can reset
        // all values again.
        gst_debug!(self.cat, obj: element, "Unlock stop");
        let mut clock_wait = self.clock_wait.lock().unwrap();
        clock_wait.flushing = false;

        true
    }
}

// This zero-sized struct is containing the static metadata of our element. It is only necessary to
// be able to implement traits on it, but e.g. a plugin that registers multiple elements with the
// same code would use this struct to store information about the concrete element. An example of
// this would be a plugin that wraps around a library that has multiple decoders with the same API,
// but wants (as it should) a separate element registered for each decoder.
struct NdiSrcStatic;

// The basic trait for registering the type: This returns a name for the type and registers the
// instance and class initializations functions with the type system, thus hooking everything
// together.
impl ImplTypeStatic<BaseSrc> for NdiSrcStatic {
    fn get_name(&self) -> &str {
        "NdiSrc"
    }

    fn new(&self, element: &BaseSrc) -> Box<BaseSrcImpl<BaseSrc>> {
        NdiSrc::new(element)
    }

    fn class_init(&self, klass: &mut BaseSrcClass) {
        NdiSrc::class_init(klass);
    }
}

// Registers the type for our element, and then registers in GStreamer under
// the name "ndisrc" for being able to instantiate it via e.g.
// gst::ElementFactory::make().
pub fn register(plugin: &gst::Plugin) {
    let type_ = register_type(NdiSrcStatic);
    gst::Element::register(plugin, "ndisrc", 0, type_);
}
