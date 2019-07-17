use glib;
use glib::subclass;
use glib::subclass::prelude::*;
use gst;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_audio;
use gst_base;
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;

use std::sync::Mutex;
use std::time;
use std::{i32, u32};

use connect_ndi;
use ndi::*;
use ndisys;
use stop_ndi;

use TimestampMode;
use DEFAULT_RECEIVER_NDI_NAME;
use HASHMAP_RECEIVERS;
#[cfg(feature = "reference-timestamps")]
use TIMECODE_CAPS;
#[cfg(feature = "reference-timestamps")]
use TIMESTAMP_CAPS;

use byte_slice_cast::AsMutSliceOf;

#[derive(Debug, Clone)]
struct Settings {
    ndi_name: Option<String>,
    ip_address: Option<String>,
    connect_timeout: u32,
    timeout: u32,
    receiver_ndi_name: String,
    bandwidth: ndisys::NDIlib_recv_bandwidth_e,
    timestamp_mode: TimestampMode,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ndi_name: None,
            ip_address: None,
            receiver_ndi_name: DEFAULT_RECEIVER_NDI_NAME.clone(),
            connect_timeout: 10000,
            timeout: 5000,
            bandwidth: ndisys::NDIlib_recv_bandwidth_highest,
            timestamp_mode: TimestampMode::ReceiveTime,
        }
    }
}

static PROPERTIES: [subclass::Property; 7] = [
    subclass::Property("ndi-name", |name| {
        glib::ParamSpec::string(
            name,
            "NDI Name",
            "NDI stream name of the sender",
            None,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("ip-address", |name| {
        glib::ParamSpec::string(
            name,
            "IP Address",
            "IP address and port of the sender, e.g. 127.0.0.1:5961",
            None,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("receiver-ndi-name", |name| {
        glib::ParamSpec::string(
            name,
            "Receiver NDI Name",
            "NDI stream name of this receiver",
            Some(&*DEFAULT_RECEIVER_NDI_NAME),
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("connect-timeout", |name| {
        glib::ParamSpec::uint(
            name,
            "Connect Timeout",
            "Connection timeout in ms",
            0,
            u32::MAX,
            10000,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("timeout", |name| {
        glib::ParamSpec::uint(
            name,
            "Timeout",
            "Receive timeout in ms",
            0,
            u32::MAX,
            5000,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("bandwidth", |name| {
        glib::ParamSpec::int(
            name,
            "Bandwidth",
            "Bandwidth, -10 metadata-only, 10 audio-only, 100 highest",
            -10,
            100,
            100,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("timestamp-mode", |name| {
        glib::ParamSpec::enum_(
            name,
            "Timestamp Mode",
            "Timestamp information to use for outgoing PTS",
            TimestampMode::static_type(),
            TimestampMode::ReceiveTime as i32,
            glib::ParamFlags::READWRITE,
        )
    }),
];

struct State {
    info: Option<gst_audio::AudioInfo>,
    id_receiver: Option<usize>,
    current_latency: gst::ClockTime,
}

impl Default for State {
    fn default() -> State {
        State {
            info: None,
            id_receiver: None,
            current_latency: gst::CLOCK_TIME_NONE,
        }
    }
}

pub(crate) struct NdiAudioSrc {
    cat: gst::DebugCategory,
    settings: Mutex<Settings>,
    state: Mutex<State>,
}

impl ObjectSubclass for NdiAudioSrc {
    const NAME: &'static str = "NdiAudioSrc";
    type ParentType = gst_base::BaseSrc;
    type Instance = gst::subclass::ElementInstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            cat: gst::DebugCategory::new(
                "ndiaudiosrc",
                gst::DebugColorFlags::empty(),
                Some("NewTek NDI Audio Source"),
            ),
            settings: Mutex::new(Default::default()),
            state: Mutex::new(Default::default()),
        }
    }

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.set_metadata(
            "NewTek NDI Audio Source",
            "Source",
            "NewTek NDI audio source",
            "Ruben Gonzalez <rubenrua@teltek.es>, Daniel Vilar <daniel.peiteado@teltek.es>, Sebastian Dröge <sebastian@centricular.com>",
        );

        let caps = gst::Caps::new_simple(
            "audio/x-raw",
            &[
                (
                    "format",
                    &gst::List::new(&[&gst_audio::AUDIO_FORMAT_S16.to_string()]),
                ),
                ("rate", &gst::IntRange::<i32>::new(1, i32::MAX)),
                ("channels", &gst::IntRange::<i32>::new(1, i32::MAX)),
                ("layout", &"interleaved"),
            ],
        );

        let src_pad_template = gst::PadTemplate::new(
            "src",
            gst::PadDirection::Src,
            gst::PadPresence::Always,
            &caps,
        )
        .unwrap();
        klass.add_pad_template(src_pad_template);

        klass.install_properties(&PROPERTIES);
    }
}

impl ObjectImpl for NdiAudioSrc {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        self.parent_constructed(obj);

        let basesrc = obj.downcast_ref::<gst_base::BaseSrc>().unwrap();
        // Initialize live-ness and notify the base class that
        // we'd like to operate in Time format
        basesrc.set_live(true);
        basesrc.set_format(gst::Format::Time);
    }

    fn set_property(&self, obj: &glib::Object, id: usize, value: &glib::Value) {
        let prop = &PROPERTIES[id];
        let basesrc = obj.downcast_ref::<gst_base::BaseSrc>().unwrap();

        match *prop {
            subclass::Property("ndi-name", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let ndi_name = value.get();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing ndi-name from {:?} to {:?}",
                    settings.ndi_name,
                    ndi_name,
                );
                settings.ndi_name = ndi_name;
            }
            subclass::Property("ip-address", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let ip_address = value.get();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing ip from {:?} to {:?}",
                    settings.ip_address,
                    ip_address,
                );
                settings.ip_address = ip_address;
            }
            subclass::Property("receiver-ndi-name", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let receiver_ndi_name = value.get();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing receiver-ndi-name from {:?} to {:?}",
                    settings.receiver_ndi_name,
                    receiver_ndi_name,
                );
                settings.receiver_ndi_name =
                    receiver_ndi_name.unwrap_or_else(|| DEFAULT_RECEIVER_NDI_NAME.clone());
            }
            subclass::Property("connect-timeout", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let connect_timeout = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing connect-timeout from {} to {}",
                    settings.connect_timeout,
                    connect_timeout,
                );
                settings.connect_timeout = connect_timeout;
            }
            subclass::Property("timeout", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let timeout = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing timeout from {} to {}",
                    settings.timeout,
                    timeout,
                );
                settings.timeout = timeout;
            }
            subclass::Property("bandwidth", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let bandwidth = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing bandwidth from {} to {}",
                    settings.bandwidth,
                    bandwidth,
                );
                settings.bandwidth = bandwidth;
            }
            subclass::Property("timestamp-mode", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let timestamp_mode = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing timestamp mode from {:?} to {:?}",
                    settings.timestamp_mode,
                    timestamp_mode
                );
                if settings.timestamp_mode != timestamp_mode {
                    let _ = basesrc
                        .post_message(&gst::Message::new_latency().src(Some(basesrc)).build());
                }
                settings.timestamp_mode = timestamp_mode;
            }
            _ => unimplemented!(),
        }
    }

    fn get_property(&self, _obj: &glib::Object, id: usize) -> Result<glib::Value, ()> {
        let prop = &PROPERTIES[id];

        match *prop {
            subclass::Property("ndi-name", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.ndi_name.to_value())
            }
            subclass::Property("ip-address", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.ip_address.to_value())
            }
            subclass::Property("receiver-ndi-name", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.receiver_ndi_name.to_value())
            }
            subclass::Property("connect-timeout", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.connect_timeout.to_value())
            }
            subclass::Property("timeout", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.timeout.to_value())
            }
            subclass::Property("bandwidth", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.bandwidth.to_value())
            }
            subclass::Property("timestamp-mode", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.timestamp_mode.to_value())
            }
            _ => unimplemented!(),
        }
    }
}

impl ElementImpl for NdiAudioSrc {}

impl BaseSrcImpl for NdiAudioSrc {
    fn start(&self, element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        *self.state.lock().unwrap() = Default::default();

        let settings = self.settings.lock().unwrap().clone();
        let mut state = self.state.lock().unwrap();

        state.id_receiver = connect_ndi(
            self.cat,
            element,
            settings.ip_address.as_ref().map(String::as_str),
            settings.ndi_name.as_ref().map(String::as_str),
            &settings.receiver_ndi_name,
            settings.connect_timeout,
            settings.bandwidth,
        );

        match state.id_receiver {
            None => Err(gst_error_msg!(
                gst::ResourceError::NotFound,
                ["Could not connect to this source"]
            )),
            _ => Ok(()),
        }
    }

    fn stop(&self, element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        *self.state.lock().unwrap() = Default::default();

        let mut state = self.state.lock().unwrap();
        if let Some(id_receiver) = state.id_receiver.take() {
            stop_ndi(self.cat, element, id_receiver);
        }
        *state = State::default();
        Ok(())
    }

    fn query(&self, element: &gst_base::BaseSrc, query: &mut gst::QueryRef) -> bool {
        use gst::QueryView;

        match query.view_mut() {
            QueryView::Scheduling(ref mut q) => {
                q.set(gst::SchedulingFlags::SEQUENTIAL, 1, -1, 0);
                q.add_scheduling_modes(&[gst::PadMode::Push]);
                true
            }
            QueryView::Latency(ref mut q) => {
                let state = self.state.lock().unwrap();
                let settings = self.settings.lock().unwrap();

                if state.current_latency.is_some() {
                    let latency = if settings.timestamp_mode == TimestampMode::Timestamp {
                        state.current_latency
                    } else {
                        0.into()
                    };

                    gst_debug!(self.cat, obj: element, "Returning latency {}", latency);
                    q.set(true, latency, gst::CLOCK_TIME_NONE);
                    true
                } else {
                    false
                }
            }
            _ => BaseSrcImplExt::parent_query(self, element, query),
        }
    }

    fn fixate(&self, element: &gst_base::BaseSrc, caps: gst::Caps) -> gst::Caps {
        let mut caps = gst::Caps::truncate(caps);
        {
            let caps = caps.make_mut();
            let s = caps.get_mut_structure(0).unwrap();
            s.fixate_field_nearest_int("rate", 48_000);
            s.fixate_field_nearest_int("channels", 2);
        }

        self.parent_fixate(element, caps)
    }

    fn create(
        &self,
        element: &gst_base::BaseSrc,
        _offset: u64,
        _length: u32,
    ) -> Result<gst::Buffer, gst::FlowError> {
        self.capture(element)
    }
}

impl NdiAudioSrc {
    fn capture(&self, element: &gst_base::BaseSrc) -> Result<gst::Buffer, gst::FlowError> {
        let settings = self.settings.lock().unwrap().clone();

        let recv = {
            let state = self.state.lock().unwrap();
            let receivers = HASHMAP_RECEIVERS.lock().unwrap();
            let receiver = &receivers.get(&state.id_receiver.unwrap()).unwrap();
            receiver.ndi_instance.clone()
        };

        let timeout = time::Instant::now();
        let audio_frame = loop {
            // FIXME: make interruptable
            let res = loop {
                match recv.capture(false, true, false, 50) {
                    Err(_) => break Err(()),
                    Ok(None) => break Ok(None),
                    Ok(Some(Frame::Audio(frame))) => break Ok(Some(frame)),
                    _ => unreachable!(),
                }
            };

            let audio_frame = match res {
                Err(_) => {
                    gst_element_error!(element, gst::ResourceError::Read, ["NDI frame type error received, assuming that the source closed the stream...."]);
                    return Err(gst::FlowError::Error);
                }
                Ok(None) if timeout.elapsed().as_millis() >= settings.timeout as u128 => {
                    return Err(gst::FlowError::Eos);
                }
                Ok(None) => {
                    gst_debug!(self.cat, obj: element, "No audio frame received yet, retry");
                    continue;
                }
                Ok(Some(frame)) => frame,
            };

            break audio_frame;
        };

        let pts = self.calculate_timestamp(element, &settings, &audio_frame);
        let info = self.create_audio_info(element, &audio_frame)?;

        {
            let mut state = self.state.lock().unwrap();
            if state.info.as_ref() != Some(&info) {
                let caps = info.to_caps().unwrap();
                state.info = Some(info.clone());
                state.current_latency = gst::SECOND
                    .mul_div_ceil(
                        audio_frame.no_samples() as u64,
                        audio_frame.sample_rate() as u64,
                    )
                    .unwrap_or(gst::CLOCK_TIME_NONE);
                drop(state);
                gst_debug!(self.cat, obj: element, "Configuring for caps {}", caps);
                element
                    .set_caps(&caps)
                    .map_err(|_| gst::FlowError::NotNegotiated)?;

                let _ =
                    element.post_message(&gst::Message::new_latency().src(Some(element)).build());
            }
        }

        let buffer = self.create_buffer(element, pts, &info, &audio_frame)?;

        gst_log!(self.cat, obj: element, "Produced buffer {:?}", buffer);

        Ok(buffer)
    }

    fn calculate_timestamp(
        &self,
        element: &gst_base::BaseSrc,
        settings: &Settings,
        audio_frame: &AudioFrame,
    ) -> gst::ClockTime {
        let clock = element.get_clock().unwrap();

        // For now take the current running time as PTS. At a later time we
        // will want to work with the timestamp given by the NDI SDK if available
        let now = clock.get_time();
        let base_time = element.get_base_time();
        let receive_time = now - base_time;

        let real_time_now = gst::ClockTime::from(glib::get_real_time() as u64 * 1000);
        let timestamp = if audio_frame.timestamp() == ndisys::NDIlib_recv_timestamp_undefined {
            gst::CLOCK_TIME_NONE
        } else {
            gst::ClockTime::from(audio_frame.timestamp() as u64 * 100)
        };
        let timecode = gst::ClockTime::from(audio_frame.timecode() as u64 * 100);

        gst_log!(
            self.cat,
            obj: element,
            "NDI audio frame received: {:?} with timecode {} and timestamp {}, receive time {}, local time now {}",
            audio_frame,
            timecode,
            timestamp,
            receive_time,
            real_time_now,
        );

        let pts = match settings.timestamp_mode {
            TimestampMode::ReceiveTime => receive_time,
            TimestampMode::Timecode => timecode,
            TimestampMode::Timestamp if timestamp.is_none() => receive_time,
            TimestampMode::Timestamp => {
                // Timestamps are relative to the UNIX epoch
                if real_time_now > timestamp {
                    let diff = real_time_now - timestamp;
                    if diff > receive_time {
                        0.into()
                    } else {
                        receive_time - diff
                    }
                } else {
                    let diff = timestamp - real_time_now;
                    receive_time + diff
                }
            }
        };

        gst_log!(
            self.cat,
            obj: element,
            "Calculated pts for audio frame: {:?}",
            pts
        );

        pts
    }

    fn create_audio_info(
        &self,
        _element: &gst_base::BaseSrc,
        audio_frame: &AudioFrame,
    ) -> Result<gst_audio::AudioInfo, gst::FlowError> {
        let builder = gst_audio::AudioInfo::new(
            gst_audio::AUDIO_FORMAT_S16,
            audio_frame.sample_rate() as u32,
            audio_frame.no_channels() as u32,
        );

        Ok(builder.build().unwrap())
    }

    fn create_buffer(
        &self,
        _element: &gst_base::BaseSrc,
        pts: gst::ClockTime,
        info: &gst_audio::AudioInfo,
        audio_frame: &AudioFrame,
    ) -> Result<gst::Buffer, gst::FlowError> {
        // We multiply by 2 because is the size in bytes of an i16 variable
        let buff_size = (audio_frame.no_samples() as u32 * info.bpf()) as usize;
        let mut buffer = gst::Buffer::with_size(buff_size).unwrap();
        {
            let duration = gst::SECOND
                .mul_div_floor(
                    audio_frame.no_samples() as u64,
                    audio_frame.sample_rate() as u64,
                )
                .unwrap_or(gst::CLOCK_TIME_NONE);
            let buffer = buffer.get_mut().unwrap();

            buffer.set_pts(pts);
            buffer.set_duration(duration);

            #[cfg(feature = "reference-timestamps")]
            {
                gst::ReferenceTimestampMeta::add(
                    buffer,
                    &*TIMECODE_CAPS,
                    gst::ClockTime::from(audio_frame.timecode() as u64 * 100),
                    gst::CLOCK_TIME_NONE,
                );
                if audio_frame.timestamp() != ndisys::NDIlib_recv_timestamp_undefined {
                    gst::ReferenceTimestampMeta::add(
                        buffer,
                        &*TIMESTAMP_CAPS,
                        gst::ClockTime::from(audio_frame.timestamp() as u64 * 100),
                        gst::CLOCK_TIME_NONE,
                    );
                }
            }

            audio_frame.copy_to_interleaved_16s(
                buffer
                    .map_writable()
                    .unwrap()
                    .as_mut_slice_of::<i16>()
                    .unwrap(),
            );
        }

        Ok(buffer)
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndiaudiosrc",
        gst::Rank::None,
        NdiAudioSrc::get_type(),
    )
}
