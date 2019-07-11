use glib;
use glib::subclass;
use glib::subclass::prelude::*;
use gst;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base;
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;

use gst::Fraction;
use gst_video;

use std::sync::Mutex;
use std::{i32, u32};

use ndi::*;

use connect_ndi;
use stop_ndi;

use NDI_STRUCT;
use HASHMAP_RECEIVERS;

#[derive(Debug, Clone)]
struct Settings {
    stream_name: String,
    ip: String,
    loss_threshold: u32,
    // FIXME: should be in state
    id_receiver: Option<usize>,
    latency: Option<gst::ClockTime>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            stream_name: String::from("Fixed ndi stream name"),
            ip: String::from(""),
            loss_threshold: 5,
            id_receiver: None,
            latency: None,
        }
    }
}

static PROPERTIES: [subclass::Property; 3] = [
    subclass::Property("stream-name", |_| {
        glib::ParamSpec::string(
            "stream-name",
            "Stream Name",
            "Name of the streaming device",
            None,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("ip", |_| {
        glib::ParamSpec::string(
            "ip",
            "Stream IP",
            "IP of the streaming device. Ex: 127.0.0.1:5961",
            None,
            glib::ParamFlags::READWRITE,
        )
    }),
    subclass::Property("loss-threshold", |_| {
        glib::ParamSpec::uint(
            "loss-threshold",
            "Loss threshold",
            "Loss threshold",
            0,
            60,
            5,
            glib::ParamFlags::READWRITE,
        )
    }),
];

struct State {
    info: Option<gst_video::VideoInfo>,
}

impl Default for State {
    fn default() -> State {
        State { info: None }
    }
}

struct TimestampData {
    offset: u64,
}

struct NdiVideoSrc {
    cat: gst::DebugCategory,
    settings: Mutex<Settings>,
    state: Mutex<State>,
    timestamp_data: Mutex<TimestampData>,
}

impl ObjectSubclass for NdiVideoSrc {
    const NAME: &'static str = "NdiVideoSrc";
    type ParentType = gst_base::BaseSrc;
    type Instance = gst::subclass::ElementInstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            cat: gst::DebugCategory::new(
                "ndivideosrc",
                gst::DebugColorFlags::empty(),
                Some("NewTek NDI Video Source"),
            ),
            settings: Mutex::new(Default::default()),
            state: Mutex::new(Default::default()),
            timestamp_data: Mutex::new(TimestampData { offset: 0 }),
        }
    }

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.set_metadata(
            "NewTek NDI Video Source",
            "Source",
            "NewTek NDI video source",
            "Ruben Gonzalez <rubenrua@teltek.es>, Daniel Vilar <daniel.peiteado@teltek.es>",
        );

        // On the src pad, we can produce F32/F64 with any sample rate
        // and any number of channels
        let caps = gst::Caps::new_simple(
            "video/x-raw",
            &[
                (
                    "format",
                    &gst::List::new(&[
                        //TODO add all formats
                        &gst_video::VideoFormat::Uyvy.to_string(),
                        //&gst_video::VideoFormat::Rgb.to_string(),
                        //&gst_video::VideoFormat::Gray8.to_string(),
                    ]),
                ),
                ("width", &gst::IntRange::<i32>::new(0, i32::MAX)),
                ("height", &gst::IntRange::<i32>::new(0, i32::MAX)),
                (
                    "framerate",
                    &gst::FractionRange::new(
                        gst::Fraction::new(0, 1),
                        gst::Fraction::new(i32::MAX, 1),
                    ),
                ),
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

impl ObjectImpl for NdiVideoSrc {
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
            subclass::Property("stream-name", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let stream_name = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing stream-name from {} to {}",
                    settings.stream_name,
                    stream_name
                );
                settings.stream_name = stream_name;
                drop(settings);
            }
            subclass::Property("ip", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let ip = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing ip from {} to {}",
                    settings.ip,
                    ip
                );
                settings.ip = ip;
                drop(settings);
            }
            subclass::Property("loss-threshold", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let loss_threshold = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing loss threshold from {} to {}",
                    settings.loss_threshold,
                    loss_threshold
                );
                settings.loss_threshold = loss_threshold;
                drop(settings);
            }
            _ => unimplemented!(),
        }
    }

    fn get_property(&self, _obj: &glib::Object, id: usize) -> Result<glib::Value, ()> {
        let prop = &PROPERTIES[id];

        match *prop {
            subclass::Property("stream-name", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.stream_name.to_value())
            }
            subclass::Property("ip", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.ip.to_value())
            }
            subclass::Property("loss-threshold", ..) => {
                let settings = self.settings.lock().unwrap();
                Ok(settings.loss_threshold.to_value())
            }
            _ => unimplemented!(),
        }
    }
}

impl ElementImpl for NdiVideoSrc {
    fn change_state(
        &self,
        element: &gst::Element,
        transition: gst::StateChange,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        if transition == gst::StateChange::PausedToPlaying {
            let mut receivers = HASHMAP_RECEIVERS.lock().unwrap();
            let settings = self.settings.lock().unwrap();

            let receiver = receivers.get_mut(&settings.id_receiver.unwrap()).unwrap();
            let recv = &receiver.ndi_instance;

            // FIXME error handling, make interruptable
            let video_frame =
            loop {
                match recv.capture(true, false, false, 1000) {
                    Err(_) => unimplemented!(),
                    Ok(None) => continue,
                    Ok(Some(Frame::Video(frame))) => break frame,
                    _ => unreachable!(),
                }
            };

            gst_debug!(
                self.cat,
                obj: element,
                "NDI video frame received: {:?}",
                video_frame
            );

            // FIXME handle unset timestamp

            if receiver.initial_timestamp <= video_frame.timestamp() as u64
                || receiver.initial_timestamp == 0
            {
                receiver.initial_timestamp = video_frame.timestamp() as u64;
            }
            gst_debug!(
                self.cat,
                obj: element,
                "Setting initial timestamp to {}",
                receiver.initial_timestamp
            );
        }
        self.parent_change_state(element, transition)
    }
}

impl BaseSrcImpl for NdiVideoSrc {
    fn set_caps(
        &self,
        element: &gst_base::BaseSrc,
        caps: &gst::Caps,
    ) -> Result<(), gst::LoggableError> {
        let info = match gst_video::VideoInfo::from_caps(caps) {
            None => {
                return Err(gst_loggable_error!(
                    self.cat,
                    "Failed to build `VideoInfo` from caps {}",
                    caps
                ));
            }
            Some(info) => info,
        };
        gst_debug!(self.cat, obj: element, "Configuring for caps {}", caps);

        let mut state = self.state.lock().unwrap();
        state.info = Some(info);
        let _ = element.post_message(&gst::Message::new_latency().src(Some(element)).build());
        Ok(())
    }

    fn start(&self, element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        *self.state.lock().unwrap() = Default::default();
        let mut settings = self.settings.lock().unwrap();
        settings.id_receiver = connect_ndi(
            self.cat,
            element,
            &settings.ip,
            &settings.stream_name,
        );

        // settings.id_receiver exists
        match settings.id_receiver {
            None => Err(gst_error_msg!(
                gst::ResourceError::NotFound,
                ["Could not connect to this source"]
            )),
            _ => Ok(()),
        }
    }

    fn stop(&self, element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        *self.state.lock().unwrap() = Default::default();

        let settings = self.settings.lock().unwrap();
        stop_ndi(self.cat, element, settings.id_receiver.unwrap());
        // Commented because when adding ndi destroy stopped in this line
        //*self.state.lock().unwrap() = Default::default();
        Ok(())
    }

    fn query(&self, element: &gst_base::BaseSrc, query: &mut gst::QueryRef) -> bool {
        use gst::QueryView;
        if let QueryView::Scheduling(ref mut q) = query.view_mut() {
            q.set(gst::SchedulingFlags::SEQUENTIAL, 1, -1, 0);
            q.add_scheduling_modes(&[gst::PadMode::Push]);
            return true;
        }
        if let QueryView::Latency(ref mut q) = query.view_mut() {
            let settings = &*self.settings.lock().unwrap();
            let state = self.state.lock().unwrap();

            if let Some(ref _info) = state.info {
                let latency = settings.latency.unwrap();
                gst_debug!(self.cat, obj: element, "Returning latency {}", latency);
                q.set(true, latency, gst::CLOCK_TIME_NONE);
                return true;
            } else {
                return false;
            }
        }
        BaseSrcImplExt::parent_query(self, element, query)
    }

    fn fixate(&self, element: &gst_base::BaseSrc, caps: gst::Caps) -> gst::Caps {
        let receivers = HASHMAP_RECEIVERS.lock().unwrap();
        let mut settings = self.settings.lock().unwrap();

        let receiver = receivers.get(&settings.id_receiver.unwrap()).unwrap();
        let recv = &receiver.ndi_instance;

        // FIXME: Should be done in create() and caps be updated as needed
        let video_frame =
        loop {
            match recv.capture(true, false, false, 1000) {
                Err(_) => unimplemented!(),
                Ok(None) => continue,
                Ok(Some(Frame::Video(frame))) => break frame,
                _ => unreachable!(),
            }
        };

        // FIXME: Why?
        settings.latency = gst::SECOND.mul_div_floor(
            video_frame.frame_rate().1 as u64,
            video_frame.frame_rate().0 as u64,
        );

        let mut caps = gst::Caps::truncate(caps);
        {
            let caps = caps.make_mut();
            let s = caps.get_mut_structure(0).unwrap();
            s.fixate_field_nearest_int("width", video_frame.xres());
            s.fixate_field_nearest_int("height", video_frame.yres());
            s.fixate_field_nearest_fraction(
                "framerate",
                Fraction::new(video_frame.frame_rate().0, video_frame.frame_rate().1),
            );
        }
        let _ = element.post_message(&gst::Message::new_latency().src(Some(element)).build());
        self.parent_fixate(element, caps)
    }

    //Creates the video buffers
    fn create(
        &self,
        element: &gst_base::BaseSrc,
        _offset: u64,
        _length: u32,
    ) -> Result<gst::Buffer, gst::FlowError> {
        let _settings = &*self.settings.lock().unwrap();

        let mut timestamp_data = self.timestamp_data.lock().unwrap();
        let state = self.state.lock().unwrap();
        let _info = match state.info {
            None => {
                gst_element_error!(element, gst::CoreError::Negotiation, ["Have no caps yet"]);
                return Err(gst::FlowError::NotNegotiated);
            }
            Some(ref info) => info.clone(),
        };
        let receivers = HASHMAP_RECEIVERS.lock().unwrap();

        let receiver = &receivers.get(&_settings.id_receiver.unwrap()).unwrap();
        let recv = &receiver.ndi_instance;

        let time = receiver.initial_timestamp;

        let mut count_frame_none = 0;
        let video_frame =
        loop {
            // FIXME: make interruptable
            let res =
            loop {
                match recv.capture(true, false, false, 1000) {
                    Err(_) => break Err(()),
                    Ok(None) => break Ok(None),
                    Ok(Some(Frame::Video(frame))) => break Ok(Some(frame)),
                    _ => unreachable!(),
                }
            };

            let video_frame = match res {
                Err(_) => {
                    gst_element_error!(element, gst::ResourceError::Read, ["NDI frame type error received, assuming that the source closed the stream...."]);
                    return Err(gst::FlowError::Error);
                },
                Ok(None) if _settings.loss_threshold != 0 => {
                    if count_frame_none < _settings.loss_threshold {
                        count_frame_none += 1;
                        continue;
                    }
                    gst_element_error!(element, gst::ResourceError::Read, ["NDI frame type none received, assuming that the source closed the stream...."]);
                    return Err(gst::FlowError::Error);
                },
                Ok(None) => {
                    gst_debug!(
                        self.cat,
                        obj: element,
                        "No video frame received, retry"
                    );
                    count_frame_none += 1;
                    continue;
                },
                Ok(Some(frame)) => frame,
            };

            if time >= (video_frame.timestamp() as u64) {
                gst_debug!(self.cat, obj: element, "Frame timestamp ({:?}) is lower than received in the first frame from NDI ({:?}), so skiping...", (video_frame.timestamp() as u64), time);
            } else {
                break video_frame;
            }
        };

        gst_log!(
            self.cat,
            obj: element,
            "NDI video frame received: {:?}",
            video_frame
        );

        let pts = video_frame.timestamp() as u64 - time;

        gst_log!(
            self.cat,
            obj: element,
            "Calculated pts for video frame: {:?}",
            pts
        );

        let buff_size = (video_frame.yres() * video_frame.line_stride_in_bytes()) as usize;
        let mut buffer = gst::Buffer::with_size(buff_size).unwrap();
        {
            // Newtek NDI yields times in 100ns intervals since the Unix Time
            let pts: gst::ClockTime = (pts * 100).into();

            let duration: gst::ClockTime = (((f64::from(video_frame.frame_rate().1)
                / f64::from(video_frame.frame_rate().0))
                * 1_000_000_000.0) as u64)
                .into();
            let buffer = buffer.get_mut().unwrap();

            // FIXME don't use static mut, also this calculation is wrong
            unsafe {
                if NDI_STRUCT.start_pts == gst::ClockTime(Some(0)) {
                    NDI_STRUCT.start_pts =
                        element.get_clock().unwrap().get_time() - element.get_base_time();
                }

                buffer.set_pts(pts + NDI_STRUCT.start_pts);
            }
            buffer.set_duration(duration);
            buffer.set_offset(timestamp_data.offset);
            timestamp_data.offset += 1;
            buffer.set_offset_end(timestamp_data.offset);
            buffer.copy_from_slice(0, video_frame.data()).unwrap();
        }

        gst_log!(self.cat, obj: element, "Produced buffer {:?}", buffer);

        Ok(buffer)
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndivideosrc",
        gst::Rank::None,
        NdiVideoSrc::get_type(),
    )
}
