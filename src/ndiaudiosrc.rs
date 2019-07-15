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
use std::{i32, u32};

use connect_ndi;
use ndi::*;
use ndisys;
use stop_ndi;

use HASHMAP_RECEIVERS;

use byte_slice_cast::AsMutSliceOf;

#[derive(Debug, Clone)]
struct Settings {
    stream_name: String,
    ip: String,
    loss_threshold: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            stream_name: String::from("Fixed ndi stream name"),
            ip: String::from(""),
            loss_threshold: 5,
        }
    }
}

static PROPERTIES: [subclass::Property; 3] = [
    subclass::Property("stream-name", |_| {
        glib::ParamSpec::string(
            "stream-name",
            "Sream Name",
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
    info: Option<gst_audio::AudioInfo>,
    id_receiver: Option<usize>,
}

impl Default for State {
    fn default() -> State {
        State {
            info: None,
            id_receiver: None,
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
            "Ruben Gonzalez <rubenrua@teltek.es>, Daniel Vilar <daniel.peiteado@teltek.es>",
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

impl ElementImpl for NdiAudioSrc {}

impl BaseSrcImpl for NdiAudioSrc {
    fn start(&self, element: &gst_base::BaseSrc) -> Result<(), gst::ErrorMessage> {
        *self.state.lock().unwrap() = Default::default();

        let settings = self.settings.lock().unwrap().clone();
        let mut state = self.state.lock().unwrap();
        state.id_receiver = connect_ndi(
            self.cat,
            element,
            &settings.ip.clone(),
            &settings.stream_name.clone(),
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
        // FIXME: Make sure to not have any mutexes locked while wait
        let settings = self.settings.lock().unwrap().clone();
        let mut state = self.state.lock().unwrap();
        let receivers = HASHMAP_RECEIVERS.lock().unwrap();

        let receiver = &receivers.get(&state.id_receiver.unwrap()).unwrap();
        let recv = &receiver.ndi_instance;

        let clock = element.get_clock().unwrap();

        let mut count_frame_none = 0;
        let audio_frame = loop {
            // FIXME: make interruptable
            let res = loop {
                match recv.capture(false, true, false, 1000) {
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
                Ok(None) if settings.loss_threshold != 0 => {
                    if count_frame_none < settings.loss_threshold {
                        count_frame_none += 1;
                        continue;
                    }
                    gst_element_error!(element, gst::ResourceError::Read, ["NDI frame type none received, assuming that the source closed the stream...."]);
                    return Err(gst::FlowError::Error);
                }
                Ok(None) => {
                    gst_debug!(self.cat, obj: element, "No audio frame received, retry");
                    count_frame_none += 1;
                    continue;
                }
                Ok(Some(frame)) => frame,
            };

            break audio_frame;
        };

        // For now take the current running time as PTS. At a later time we
        // will want to work with the timestamp given by the NDI SDK if available
        let now = clock.get_time();
        let base_time = element.get_base_time();
        let pts = now - base_time;

        gst_log!(
            self.cat,
            obj: element,
            "NDI audio frame received: {:?} with timecode {} and timestamp {}",
            audio_frame,
            if audio_frame.timecode() == ndisys::NDIlib_send_timecode_synthesize {
                gst::CLOCK_TIME_NONE
            } else {
                gst::ClockTime::from(audio_frame.timecode() as u64 * 100)
            },
            if audio_frame.timestamp() == ndisys::NDIlib_recv_timestamp_undefined {
                gst::CLOCK_TIME_NONE
            } else {
                gst::ClockTime::from(audio_frame.timestamp() as u64 * 100)
            },
        );

        let info = gst_audio::AudioInfo::new(
            gst_audio::AUDIO_FORMAT_S16,
            audio_frame.sample_rate() as u32,
            audio_frame.no_channels() as u32,
        )
        .build()
        .unwrap();

        if state.info.as_ref() != Some(&info) {
            let caps = info.to_caps().unwrap();
            state.info = Some(info);
            gst_debug!(self.cat, obj: element, "Configuring for caps {}", caps);
            element
                .set_caps(&caps)
                .map_err(|_| gst::FlowError::NotNegotiated)?;
        }

        gst_log!(
            self.cat,
            obj: element,
            "Calculated pts for audio frame: {:?}",
            pts
        );

        // We multiply by 2 because is the size in bytes of an i16 variable
        let buff_size = (audio_frame.no_samples() * 2 * audio_frame.no_channels()) as usize;
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

            audio_frame.copy_to_interleaved_16s(
                buffer
                    .map_writable()
                    .unwrap()
                    .as_mut_slice_of::<i16>()
                    .unwrap(),
            );
        }

        gst_log!(self.cat, obj: element, "Produced buffer {:?}", buffer);

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
