use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::{gst_debug, gst_error};
use gst_base::prelude::*;
use gst_base::subclass::base_src::CreateSuccess;
use gst_base::subclass::prelude::*;

use std::sync::Mutex;
use std::{i32, u32};

use once_cell::sync::Lazy;

use crate::connect_ndi;
use crate::ndisys;

use crate::AudioReceiver;
use crate::Receiver;
use crate::ReceiverControlHandle;
use crate::ReceiverItem;
use crate::TimestampMode;
use crate::DEFAULT_RECEIVER_NDI_NAME;

#[derive(Debug, Clone)]
struct Settings {
    ndi_name: Option<String>,
    url_address: Option<String>,
    connect_timeout: u32,
    timeout: u32,
    max_queue_length: u32,
    receiver_ndi_name: String,
    bandwidth: ndisys::NDIlib_recv_bandwidth_e,
    timestamp_mode: TimestampMode,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ndi_name: None,
            url_address: None,
            receiver_ndi_name: DEFAULT_RECEIVER_NDI_NAME.clone(),
            connect_timeout: 10000,
            timeout: 5000,
            max_queue_length: 5,
            bandwidth: ndisys::NDIlib_recv_bandwidth_highest,
            timestamp_mode: TimestampMode::ReceiveTimeTimecode,
        }
    }
}

struct State {
    info: Option<gst_audio::AudioInfo>,
    receiver: Option<Receiver<AudioReceiver>>,
    current_latency: Option<gst::ClockTime>,
}

impl Default for State {
    fn default() -> State {
        State {
            info: None,
            receiver: None,
            current_latency: gst::ClockTime::NONE,
        }
    }
}

pub struct NdiAudioSrc {
    cat: gst::DebugCategory,
    settings: Mutex<Settings>,
    state: Mutex<State>,
    receiver_controller: Mutex<Option<ReceiverControlHandle<AudioReceiver>>>,
}

#[glib::object_subclass]
impl ObjectSubclass for NdiAudioSrc {
    const NAME: &'static str = "NdiAudioSrc";
    type Type = super::NdiAudioSrc;
    type ParentType = gst_base::BaseSrc;

    fn new() -> Self {
        Self {
            cat: gst::DebugCategory::new(
                "ndiaudiosrc",
                gst::DebugColorFlags::empty(),
                Some("NewTek NDI Audio Source"),
            ),
            settings: Mutex::new(Default::default()),
            state: Mutex::new(Default::default()),
            receiver_controller: Mutex::new(None),
        }
    }
}

impl ObjectImpl for NdiAudioSrc {
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
            vec![
                glib::ParamSpec::new_string(
                    "ndi-name",
                    "NDI Name",
                    "NDI stream name of the sender",
                    None,
                    glib::ParamFlags::READWRITE,
                ),
                glib::ParamSpec::new_string(
                    "url-address",
                    "URL/Address",
                    "URL/address and port of the sender, e.g. 127.0.0.1:5961",
                    None,
                    glib::ParamFlags::READWRITE,
                ),
                glib::ParamSpec::new_string(
                    "receiver-ndi-name",
                    "Receiver NDI Name",
                    "NDI stream name of this receiver",
                    Some(&*DEFAULT_RECEIVER_NDI_NAME),
                    glib::ParamFlags::READWRITE,
                ),
                glib::ParamSpec::new_uint(
                    "connect-timeout",
                    "Connect Timeout",
                    "Connection timeout in ms",
                    0,
                    u32::MAX,
                    10000,
                    glib::ParamFlags::READWRITE,
                ),
                glib::ParamSpec::new_uint(
                    "timeout",
                    "Timeout",
                    "Receive timeout in ms",
                    0,
                    u32::MAX,
                    5000,
                    glib::ParamFlags::READWRITE,
                ),
                glib::ParamSpec::new_uint(
                    "max-queue-length",
                    "Max Queue Length",
                    "Maximum receive queue length",
                    0,
                    u32::MAX,
                    5,
                    glib::ParamFlags::READWRITE,
                ),
                glib::ParamSpec::new_int(
                    "bandwidth",
                    "Bandwidth",
                    "Bandwidth, -10 metadata-only, 10 audio-only, 100 highest",
                    -10,
                    100,
                    100,
                    glib::ParamFlags::READWRITE,
                ),
                glib::ParamSpec::new_enum(
                    "timestamp-mode",
                    "Timestamp Mode",
                    "Timestamp information to use for outgoing PTS",
                    TimestampMode::static_type(),
                    TimestampMode::ReceiveTimeTimecode as i32,
                    glib::ParamFlags::READWRITE,
                ),
            ]
        });

        PROPERTIES.as_ref()
    }

    fn constructed(&self, obj: &Self::Type) {
        self.parent_constructed(obj);

        // Initialize live-ness and notify the base class that
        // we'd like to operate in Time format
        obj.set_live(true);
        obj.set_format(gst::Format::Time);
    }

    fn set_property(
        &self,
        obj: &Self::Type,
        _id: usize,
        value: &glib::Value,
        pspec: &glib::ParamSpec,
    ) {
        match pspec.name() {
            "ndi-name" => {
                let mut settings = self.settings.lock().unwrap();
                let ndi_name = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: obj,
                    "Changing ndi-name from {:?} to {:?}",
                    settings.ndi_name,
                    ndi_name,
                );
                settings.ndi_name = ndi_name;
            }
            "url-address" => {
                let mut settings = self.settings.lock().unwrap();
                let url_address = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: obj,
                    "Changing url-address from {:?} to {:?}",
                    settings.url_address,
                    url_address,
                );
                settings.url_address = url_address;
            }
            "receiver-ndi-name" => {
                let mut settings = self.settings.lock().unwrap();
                let receiver_ndi_name = value.get::<Option<String>>().unwrap();
                gst_debug!(
                    self.cat,
                    obj: obj,
                    "Changing receiver-ndi-name from {:?} to {:?}",
                    settings.receiver_ndi_name,
                    receiver_ndi_name,
                );
                settings.receiver_ndi_name =
                    receiver_ndi_name.unwrap_or_else(|| DEFAULT_RECEIVER_NDI_NAME.clone());
            }
            "connect-timeout" => {
                let mut settings = self.settings.lock().unwrap();
                let connect_timeout = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: obj,
                    "Changing connect-timeout from {} to {}",
                    settings.connect_timeout,
                    connect_timeout,
                );
                settings.connect_timeout = connect_timeout;
            }
            "timeout" => {
                let mut settings = self.settings.lock().unwrap();
                let timeout = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: obj,
                    "Changing timeout from {} to {}",
                    settings.timeout,
                    timeout,
                );
                settings.timeout = timeout;
            }
            "max-queue-length" => {
                let mut settings = self.settings.lock().unwrap();
                let max_queue_length = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: obj,
                    "Changing max-queue-length from {} to {}",
                    settings.max_queue_length,
                    max_queue_length,
                );
                settings.max_queue_length = max_queue_length;
            }
            "bandwidth" => {
                let mut settings = self.settings.lock().unwrap();
                let bandwidth = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: obj,
                    "Changing bandwidth from {} to {}",
                    settings.bandwidth,
                    bandwidth,
                );
                settings.bandwidth = bandwidth;
            }
            "timestamp-mode" => {
                let mut settings = self.settings.lock().unwrap();
                let timestamp_mode = value.get().unwrap();
                gst_debug!(
                    self.cat,
                    obj: obj,
                    "Changing timestamp mode from {:?} to {:?}",
                    settings.timestamp_mode,
                    timestamp_mode
                );
                if settings.timestamp_mode != timestamp_mode {
                    let _ = obj.post_message(gst::message::Latency::builder().src(obj).build());
                }
                settings.timestamp_mode = timestamp_mode;
            }
            _ => unimplemented!(),
        }
    }

    fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "ndi-name" => {
                let settings = self.settings.lock().unwrap();
                settings.ndi_name.to_value()
            }
            "url-address" => {
                let settings = self.settings.lock().unwrap();
                settings.url_address.to_value()
            }
            "receiver-ndi-name" => {
                let settings = self.settings.lock().unwrap();
                settings.receiver_ndi_name.to_value()
            }
            "connect-timeout" => {
                let settings = self.settings.lock().unwrap();
                settings.connect_timeout.to_value()
            }
            "timeout" => {
                let settings = self.settings.lock().unwrap();
                settings.timeout.to_value()
            }
            "max-queue-length" => {
                let settings = self.settings.lock().unwrap();
                settings.max_queue_length.to_value()
            }
            "bandwidth" => {
                let settings = self.settings.lock().unwrap();
                settings.bandwidth.to_value()
            }
            "timestamp-mode" => {
                let settings = self.settings.lock().unwrap();
                settings.timestamp_mode.to_value()
            }
            _ => unimplemented!(),
        }
    }
}

impl ElementImpl for NdiAudioSrc {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
            "NewTek NDI Audio Source",
            "Source",
            "NewTek NDI audio source",
            "Ruben Gonzalez <rubenrua@teltek.es>, Daniel Vilar <daniel.peiteado@teltek.es>, Sebastian Dröge <sebastian@centricular.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
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

            let audio_src_pad_template = gst::PadTemplate::new(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Sometimes,
                &caps,
            )
            .unwrap();

            vec![audio_src_pad_template]
        });

        PAD_TEMPLATES.as_ref()
    }

    fn change_state(
        &self,
        element: &Self::Type,
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

impl BaseSrcImpl for NdiAudioSrc {
    fn negotiate(&self, _element: &Self::Type) -> Result<(), gst::LoggableError> {
        // Always succeed here without doing anything: we will set the caps once we received a
        // buffer, there's nothing we can negotiate
        Ok(())
    }

    fn unlock(&self, element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        gst_debug!(self.cat, obj: element, "Unlocking",);
        if let Some(ref controller) = *self.receiver_controller.lock().unwrap() {
            controller.set_flushing(true);
        }
        Ok(())
    }

    fn unlock_stop(&self, element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        gst_debug!(self.cat, obj: element, "Stop unlocking",);
        if let Some(ref controller) = *self.receiver_controller.lock().unwrap() {
            controller.set_flushing(false);
        }
        Ok(())
    }

    fn start(&self, element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        *self.state.lock().unwrap() = Default::default();
        let settings = self.settings.lock().unwrap().clone();

        if settings.ndi_name.is_none() && settings.url_address.is_none() {
            return Err(gst::error_msg!(
                gst::LibraryError::Settings,
                ["No NDI name or URL/address given"]
            ));
        }

        let receiver = connect_ndi(
            self.cat,
            element.upcast_ref(),
            settings.ndi_name.as_deref(),
            settings.url_address.as_deref(),
            &settings.receiver_ndi_name,
            settings.connect_timeout,
            settings.bandwidth,
            settings.timestamp_mode,
            settings.timeout,
            settings.max_queue_length as usize,
        );

        // settings.id_receiver exists
        match receiver {
            None => Err(gst::error_msg!(
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

    fn stop(&self, _element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        if let Some(ref controller) = self.receiver_controller.lock().unwrap().take() {
            controller.shutdown();
        }
        *self.state.lock().unwrap() = State::default();
        Ok(())
    }

    fn query(&self, element: &Self::Type, query: &mut gst::QueryRef) -> bool {
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

                if let Some(latency) = state.current_latency {
                    let min = if settings.timestamp_mode != TimestampMode::Timecode {
                        latency
                    } else {
                        gst::ClockTime::ZERO
                    };

                    let max = 5 * latency;

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

    fn fixate(&self, element: &Self::Type, mut caps: gst::Caps) -> gst::Caps {
        caps.truncate();
        {
            let caps = caps.make_mut();
            let s = caps.structure_mut(0).unwrap();
            s.fixate_field_nearest_int("rate", 48_000);
            s.fixate_field_nearest_int("channels", 2);
        }

        self.parent_fixate(element, caps)
    }

    fn create(
        &self,
        element: &Self::Type,
        _offset: u64,
        _buffer: Option<&mut gst::BufferRef>,
        _length: u32,
    ) -> Result<CreateSuccess, gst::FlowError> {
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
                        gst::element_error!(
                            element,
                            gst::ResourceError::Settings,
                            ["Invalid audio info received: {:?}", info]
                        );
                        gst::FlowError::NotNegotiated
                    })?;
                    state.info = Some(info);
                    state.current_latency = buffer.duration();
                    drop(state);
                    gst_debug!(self.cat, obj: element, "Configuring for caps {}", caps);
                    element.set_caps(&caps).map_err(|_| {
                        gst::element_error!(
                            element,
                            gst::CoreError::Negotiation,
                            ["Failed to negotiate caps: {:?}", caps]
                        );
                        gst::FlowError::NotNegotiated
                    })?;

                    let _ =
                        element.post_message(gst::message::Latency::builder().src(element).build());
                }

                Ok(CreateSuccess::NewBuffer(buffer))
            }
            ReceiverItem::Flushing => Err(gst::FlowError::Flushing),
            ReceiverItem::Timeout => Err(gst::FlowError::Eos),
            ReceiverItem::Error(err) => Err(err),
        }
    }
}
