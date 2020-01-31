use glib;
use glib::subclass;
use gst;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base;
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;
use gst_video;

use std::sync::Mutex;
use std::{i32, u32};

use crate::ndisys;

use crate::connect_ndi;

use crate::Receiver;
use crate::ReceiverControlHandle;
use crate::ReceiverItem;
use crate::TimestampMode;
use crate::VideoReceiver;
use crate::DEFAULT_RECEIVER_NDI_NAME;

#[derive(Debug, Clone)]
struct Settings {
    ndi_name: Option<String>,
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
            receiver_ndi_name: DEFAULT_RECEIVER_NDI_NAME.clone(),
            connect_timeout: 10000,
            timeout: 5000,
            bandwidth: ndisys::NDIlib_recv_bandwidth_highest,
            timestamp_mode: TimestampMode::ReceiveTime,
        }
    }
}

static PROPERTIES: [subclass::Property; 6] = [
    subclass::Property("ndi-name", |name| {
        glib::ParamSpec::string(
            name,
            "NDI Name",
            "NDI stream name of the sender",
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
    info: Option<gst_video::VideoInfo>,
    current_latency: gst::ClockTime,
    receiver: Option<Receiver<VideoReceiver>>,
}

impl Default for State {
    fn default() -> State {
        State {
            info: None,
            current_latency: gst::CLOCK_TIME_NONE,
            receiver: None,
        }
    }
}

pub(crate) struct NdiVideoSrc {
    cat: gst::DebugCategory,
    settings: Mutex<Settings>,
    state: Mutex<State>,
    receiver_controller: Mutex<Option<ReceiverControlHandle<VideoReceiver>>>,
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
            receiver_controller: Mutex::new(None),
        }
    }

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.set_metadata(
            "NewTek NDI Video Source",
            "Source",
            "NewTek NDI video source",
            "Ruben Gonzalez <rubenrua@teltek.es>, Daniel Vilar <daniel.peiteado@teltek.es>, Sebastian Dröge <sebastian@centricular.com>",
        );

        // On the src pad, we can produce F32/F64 with any sample rate
        // and any number of channels
        let caps = gst::Caps::new_simple(
            "video/x-raw",
            &[
                (
                    "format",
                    &gst::List::new(&[
                        &gst_video::VideoFormat::Uyvy.to_string(),
                        &gst_video::VideoFormat::Yv12.to_string(),
                        &gst_video::VideoFormat::Nv12.to_string(),
                        &gst_video::VideoFormat::I420.to_string(),
                        &gst_video::VideoFormat::Bgra.to_string(),
                        &gst_video::VideoFormat::Bgrx.to_string(),
                        &gst_video::VideoFormat::Rgba.to_string(),
                        &gst_video::VideoFormat::Rgbx.to_string(),
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

        #[cfg(feature = "interlaced-fields")]
        let caps = {
            let mut tmp = caps.copy();
            {
                let tmp = tmp.get_mut().unwrap();
                tmp.set_features_simple(Some(gst::CapsFeatures::new(&["format:Interlaced"])));
            }

            let mut caps = caps;
            {
                let caps = caps.get_mut().unwrap();
                caps.append(tmp);
            }

            caps
        };

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
            subclass::Property("ndi-name", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let ndi_name = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: basesrc,
                    "Changing ndi-name from {:?} to {:?}",
                    settings.ndi_name,
                    ndi_name,
                );
                settings.ndi_name = ndi_name;
            }
            subclass::Property("receiver-ndi-name", ..) => {
                let mut settings = self.settings.lock().unwrap();
                let receiver_ndi_name = value.get().unwrap();
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
                let connect_timeout = value.get_some().unwrap();
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
                let timeout = value.get_some().unwrap();
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
                let bandwidth = value.get_some().unwrap();
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
                let timestamp_mode = value.get_some().unwrap();
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

impl ElementImpl for NdiVideoSrc {
    fn change_state(
        &self,
        element: &gst::Element,
        transition: gst::StateChange,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        match transition {
            gst::StateChange::PausedToPlaying => {
                if let Some(ref controller) = *self.receiver_controller.lock().unwrap() {
                    controller.set_playing(true);
                }
            }
            gst::StateChange::PlayingToPaused => {
                if let Some(ref controller) = *self.receiver_controller.lock().unwrap() {
                    controller.set_playing(false);
                }
            }
            gst::StateChange::PausedToReady => {
                if let Some(ref controller) = *self.receiver_controller.lock().unwrap() {
                    controller.shutdown();
                }
            }
            _ => (),
        }

        self.parent_change_state(element, transition)
    }
}

impl BaseSrcImpl for NdiVideoSrc {
    fn negotiate(&self, _element: &gst_base::BaseSrc) -> Result<(), gst::LoggableError> {
        // Always succeed here without doing anything: we will set the caps once we received a
        // buffer, there's nothing we can negotiate
        Ok(())
    }

    fn unlock(&self, element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        gst_debug!(self.cat, obj: element, "Unlocking",);
        if let Some(ref controller) = *self.receiver_controller.lock().unwrap() {
            controller.set_flushing(true);
        }
        Ok(())
    }

    fn unlock_stop(&self, element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        gst_debug!(self.cat, obj: element, "Stop unlocking",);
        if let Some(ref controller) = *self.receiver_controller.lock().unwrap() {
            controller.set_flushing(false);
        }
        Ok(())
    }

    fn start(&self, element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        *self.state.lock().unwrap() = Default::default();
        let settings = self.settings.lock().unwrap().clone();

        let ndi_name = if let Some(ref ndi_name) = settings.ndi_name {
            ndi_name
        } else {
            return Err(gst_error_msg!(
                gst::LibraryError::Settings,
                ["No IP address or NDI name given"]
            ));
        };

        let receiver = connect_ndi(
            self.cat,
            element,
            ndi_name,
            &settings.receiver_ndi_name,
            settings.connect_timeout,
            settings.bandwidth,
            settings.timestamp_mode,
            settings.timeout,
        );

        // settings.id_receiver exists
        match receiver {
            None => Err(gst_error_msg!(
                gst::ResourceError::NotFound,
                ["Could not connect to this source"]
            )),
            Some(receiver) => {
                *self.receiver_controller.lock().unwrap() =
                    Some(receiver.receiver_control_handle());
                let mut state = self.state.lock().unwrap();
                state.receiver = Some(receiver);

                Ok(())
            }
        }
    }

    fn stop(&self, _element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        if let Some(ref controller) = self.receiver_controller.lock().unwrap().take() {
            controller.shutdown();
        }
        *self.state.lock().unwrap() = State::default();
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
                    let min = if settings.timestamp_mode != TimestampMode::Timecode {
                        state.current_latency
                    } else {
                        0.into()
                    };

                    let max = 5 * state.current_latency;

                    gst_debug!(
                        self.cat,
                        obj: element,
                        "Returning latency min {} max {}",
                        min,
                        max
                    );
                    q.set(true, min, max);
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
            s.fixate_field_nearest_int("width", 1920);
            s.fixate_field_nearest_int("height", 1080);
            if s.has_field("pixel-aspect-ratio") {
                s.fixate_field_nearest_fraction("pixel-aspect-ratio", gst::Fraction::new(1, 1));
            }
        }

        self.parent_fixate(element, caps)
    }

    //Creates the video buffers
    fn create(
        &self,
        element: &gst_base::BaseSrc,
        _offset: u64,
        _length: u32,
    ) -> Result<gst::Buffer, gst::FlowError> {
        let recv = {
            let mut state = self.state.lock().unwrap();
            match state.receiver.take() {
                Some(recv) => recv,
                None => {
                    gst_error!(self.cat, obj: element, "Have no receiver");
                    return Err(gst::FlowError::Error);
                }
            }
        };

        match recv.capture() {
            ReceiverItem::Buffer(buffer, info) => {
                let mut state = self.state.lock().unwrap();
                state.receiver = Some(recv);
                if state.info.as_ref() != Some(&info) {
                    let caps = info.to_caps().map_err(|_| {
                        gst_element_error!(
                            element,
                            gst::ResourceError::Settings,
                            ["Invalid audio info received: {:?}", info]
                        );
                        gst::FlowError::NotNegotiated
                    })?;
                    state.info = Some(info);
                    state.current_latency = buffer.get_duration();
                    drop(state);
                    gst_debug!(self.cat, obj: element, "Configuring for caps {}", caps);
                    element.set_caps(&caps).map_err(|_| {
                        gst_element_error!(
                            element,
                            gst::CoreError::Negotiation,
                            ["Failed to negotiate caps: {:?}", caps]
                        );
                        gst::FlowError::NotNegotiated
                    })?;

                    let _ = element
                        .post_message(&gst::Message::new_latency().src(Some(element)).build());
                }

                Ok(buffer)
            }
            ReceiverItem::Timeout => Err(gst::FlowError::Eos),
            ReceiverItem::Flushing => Err(gst::FlowError::Flushing),
            ReceiverItem::Error(err) => Err(err),
        }
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
