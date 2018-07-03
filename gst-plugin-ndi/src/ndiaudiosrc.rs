#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]

use glib;
use gst;
use gst::prelude::*;
use gst_audio;
use gst_base::prelude::*;

use gst_plugin::base_src::*;
use gst_plugin::element::*;
use gst_plugin::object::*;
use gst_plugin::properties::*;

use std::sync::Mutex;
use std::{i32, u32};

use std::ptr;

use ndilib::*;
use connect_ndi;
use ndi_struct;
use stop_ndi;

// Property value storage
#[derive(Debug, Clone)]
struct Settings {
    stream_name: String,
    ip: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            stream_name: String::from("Fixed ndi stream name"),
            ip: String::from(""),
        }
    }
}

// Metadata for the properties
static PROPERTIES: [Property; 2] = [
Property::String(
    "stream-name",
    "Sream Name",
    "Name of the streaming device",
    None,
    PropertyMutability::ReadWrite,
),
Property::String(
    "ip",
    "Stream IP",
    "Stream IP",
    None,
    PropertyMutability::ReadWrite,
),
];

// Stream-specific state, i.e. audio format configuration
// and sample offset
struct State {
    info: Option<gst_audio::AudioInfo>,
}

impl Default for State {
    fn default() -> State {
        State {
            info: None,
        }
    }
}

struct TimestampData{
    pts: u64,
    offset: u64,
}

// Struct containing all the element data
struct NdiAudioSrc {
    cat: gst::DebugCategory,
    settings: Mutex<Settings>,
    state: Mutex<State>,
    timestamp_data: Mutex<TimestampData>,
}

impl NdiAudioSrc {
    // Called when a new instance is to be created
    fn new(element: &BaseSrc) -> Box<BaseSrcImpl<BaseSrc>> {
        // Initialize live-ness and notify the base class that
        // we'd like to operate in Time format
        element.set_live(true);
        element.set_format(gst::Format::Time);

        Box::new(Self {
            cat: gst::DebugCategory::new(
                "ndiaudiosrc",
                gst::DebugColorFlags::empty(),
                "NewTek NDI Audio Source",
            ),
            settings: Mutex::new(Default::default()),
            state: Mutex::new(Default::default()),
            timestamp_data: Mutex::new(TimestampData{
                pts: 0,
                offset: 0,
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
            "NewTek NDI Audio Source",
            "Source",
            "NewTek NDI audio source",
            "Ruben Gonzalez <rubenrua@teltek.es>, Daniel Vilar <daniel.peiteado@teltek.es>",
        );

        // On the src pad, we can produce F32/F64 with any sample rate
        // and any number of channels
        let caps = gst::Caps::new_simple(
            "audio/x-raw",
            &[
            (
                "format",
                &gst::List::new(&[
                    //TODO add all formats?
                    &gst_audio::AUDIO_FORMAT_F32.to_string(),
                    &gst_audio::AUDIO_FORMAT_F64.to_string(),
                    &gst_audio::AUDIO_FORMAT_S16.to_string(),
                    ]),
                ),
                ("rate", &gst::IntRange::<i32>::new(1, i32::MAX)),
                ("channels", &gst::IntRange::<i32>::new(1, i32::MAX)),
                ("layout", &"interleaved"),
                ],
            );
            // The src pad template must be named "src" for basesrc
            // and specific a pad that is always there
            let src_pad_template = gst::PadTemplate::new(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &caps,
                //&gst::Caps::new_any(),
            );
            klass.add_pad_template(src_pad_template);

            // Install all our properties
            klass.install_properties(&PROPERTIES);
        }
    }


    // Virtual methods of GObject itself
    impl ObjectImpl<BaseSrc> for NdiAudioSrc {
        // Called whenever a value of a property is changed. It can be called
        // at any time from any thread.
        fn set_property(&self, obj: &glib::Object, id: u32, value: &glib::Value) {
            let prop = &PROPERTIES[id as usize];
            let element = obj.clone().downcast::<BaseSrc>().unwrap();

            match *prop {
                Property::String("stream-name", ..) => {
                    let mut settings = self.settings.lock().unwrap();
                    let stream_name = value.get().unwrap();
                    gst_debug!(
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
                },
                Property::String("ip", ..) => {
                    let mut settings = self.settings.lock().unwrap();
                    let ip = value.get().unwrap();
                    gst_debug!(
                        self.cat,
                        obj: &element,
                        "Changing ip from {} to {}",
                        settings.ip,
                        ip
                    );
                    settings.ip = ip;
                    drop(settings);

                    let _ =
                    element.post_message(&gst::Message::new_latency().src(Some(&element)).build());
                }
                _ => unimplemented!(),
            }
        }

        // Called whenever a value of a property is read. It can be called
        // at any time from any thread.
        fn get_property(&self, _obj: &glib::Object, id: u32) -> Result<glib::Value, ()> {
            let prop = &PROPERTIES[id as usize];

            match *prop {
                Property::String("stream-name", ..) => {
                    let settings = self.settings.lock().unwrap();
                    //TODO to_value supongo que solo funciona con numeros
                    Ok(settings.stream_name.to_value())
                },
                Property::String("ip", ..) => {
                    let settings = self.settings.lock().unwrap();
                    //TODO to_value supongo que solo funciona con numeros
                    Ok(settings.ip.to_value())
                }
                _ => unimplemented!(),
            }
        }
    }

    // Virtual methods of gst::Element. We override none
    impl ElementImpl<BaseSrc> for NdiAudioSrc {
    }

    // Virtual methods of gst_base::BaseSrc
    impl BaseSrcImpl<BaseSrc> for NdiAudioSrc {
        // Called whenever the input/output caps are changing, i.e. in the very beginning before data
        // flow happens and whenever the situation in the pipeline is changing. All buffers after this
        // call have the caps given here.
        //
        // We simply remember the resulting AudioInfo from the caps to be able to use this for knowing
        // the sample rate, etc. when creating buffers
        fn set_caps(&self, element: &BaseSrc, caps: &gst::CapsRef) -> bool {

            let info = match gst_audio::AudioInfo::from_caps(caps) {
                None => return false,
                Some(info) => info,
            };

            gst_debug!(self.cat, obj: element, "Configuring for caps {}", caps);

            // TODO Puede que falle si no creamos la estructura de cero, pero si lo hacemos no podemos poner recv a none
            let mut state = self.state.lock().unwrap();
            state.info = Some(info);

            true
        }

        // Called when starting, so we can initialize all stream-related state to its defaults
        fn start(&self, element: &BaseSrc) -> bool {
            // Reset state
            *self.state.lock().unwrap() = Default::default();

            let settings = self.settings.lock().unwrap();
            return connect_ndi(self.cat, element, settings.ip.clone(), settings.stream_name.clone());

        }

        // Called when shutting down the element so we can release all stream-related state
        fn stop(&self, element: &BaseSrc) -> bool {
            // Reset state
            *self.state.lock().unwrap() = Default::default();
            stop_ndi(self.cat, element);
            // Commented because when adding ndi destroy stopped in this line
            //*self.state.lock().unwrap() = Default::default();
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
                    //let settings = *self.settings.lock().unwrap();
                    let state = self.state.lock().unwrap();

                    if let Some(ref info) = state.info {
                        // let latency = gst::SECOND
                        // .mul_div_floor(settings.samples_per_buffer as u64, info.rate() as u64)
                        // .unwrap();
                        let latency = gst::SECOND.mul_div_floor(3 as u64, 2 as u64).unwrap();
                        // let latency = gst::SECOND
                        // .mul_div_floor(1 as u64, 30 as u64)
                        // .unwrap();
                        // gst_debug!(self.cat, obj: element, "Returning latency {}", latency);
                        println!("/*/a*f/a*sd/f*ad/sf*ad/sf*ad/sf");
                        let max = latency * 1843200;
                        println!("{:?}", latency);
                        println!("{:?}",max);
                        q.set(true, latency, max);
                        return true;
                    } else {
                        return false;
                    }
                }
                _ => (),
            }
            BaseSrcBase::parent_query(element, query)
        }

        fn fixate(&self, element: &BaseSrc, caps: gst::Caps) -> gst::Caps {
            //We need to set the correct caps resolution and framerate
            unsafe{
                let recv = match ndi_struct.recv{
                    None => {
                        //TODO Update gst_element_error with one more descriptive
                        //println!("pNDI_recv no encontrado");
                        gst_element_error!(element, gst::CoreError::Negotiation, ["No encontramos ndi recv"]);
                        return caps;
                    }
                    Some(ref recv) => recv.clone(),
                };

                let pNDI_recv = recv.recv;
                let mut timestamp_data = self.timestamp_data.lock().unwrap();

                let audio_frame: NDIlib_audio_frame_v2_t = Default::default();
                let mut frame_type: NDIlib_frame_type_e = NDIlib_frame_type_e::NDIlib_frame_type_none;
                while frame_type != NDIlib_frame_type_e::NDIlib_frame_type_audio{
                    frame_type = NDIlib_recv_capture_v2(pNDI_recv, ptr::null(), &audio_frame, ptr::null(), 1000);
                }
                ndi_struct.start_pts = audio_frame.timecode as u64;
                //timestamp_data.pts = audio_frame.timecode as u64;

                let mut caps = gst::Caps::truncate(caps);
                {
                    let caps = caps.make_mut();
                    let s = caps.get_mut_structure(0).unwrap();
                    //s.fixate_field_nearest_int("rate", audio_frame.sample_rate);
                    s.fixate_field_nearest_int("rate", audio_frame.sample_rate / audio_frame.no_channels);
                    s.fixate_field_nearest_int("channels", audio_frame.no_channels);
                }

                // Let BaseSrc fixate anything else for us. We could've alternatively have
                // called Caps::fixate() here
                element.parent_fixate(caps)
            }
        }

        //Creates the audio buffers
        fn create(
            &self,
            element: &BaseSrc,
            _offset: u64,
            _length: u32,
        ) -> Result<gst::Buffer, gst::FlowReturn> {
            // Keep a local copy of the values of all our properties at this very moment. This
            // ensures that the mutex is never locked for long and the application wouldn't
            // have to block until this function returns when getting/setting property values
            let _settings = &*self.settings.lock().unwrap();

            let mut timestamp_data = self.timestamp_data.lock().unwrap();
            // Get a locked reference to our state, i.e. the input and output AudioInfo
            let state = self.state.lock().unwrap();
            let _info = match state.info {
                None => {
                    gst_element_error!(element, gst::CoreError::Negotiation, ["Have no caps yet"]);
                    return Err(gst::FlowReturn::NotNegotiated);
                }
                Some(ref info) => info.clone(),
            };
            unsafe{
                let recv = match ndi_struct.recv{
                    None => {
                        //TODO Update gst_element_error with one more descriptive
                        //println!("pNDI_recv no encontrado");
                        gst_element_error!(element, gst::CoreError::Negotiation, ["No encontramos ndi recv"]);
                        return Err(gst::FlowReturn::NotNegotiated);
                    }
                    Some(ref recv) => recv.clone(),
                };
                let pNDI_recv = recv.recv;
                let pts: u64;

                let audio_frame: NDIlib_audio_frame_v2_t = Default::default();
                NDIlib_recv_capture_v2(pNDI_recv, ptr::null(), &audio_frame, ptr::null(), 1000,);
                //pts = (audio_frame.timecode as u64) - timestamp_data.pts;
                pts = (audio_frame.timecode as u64) - ndi_struct.start_pts;

                let buff_size = ((audio_frame.channel_stride_in_bytes)) as usize;
                let mut buffer = gst::Buffer::with_size(buff_size).unwrap();
                {
                    let  vec = Vec::from_raw_parts(audio_frame.p_data as *mut u8, buff_size, buff_size);
                    //TODO Set pts, duration and other info about the buffer
                    let pts: gst::ClockTime = (pts * 100).into();
                    let duration: gst::ClockTime = (((audio_frame.no_samples as f64 / audio_frame.sample_rate as f64) * 10000000.0) as u64).into();
                    let buffer = buffer.get_mut().unwrap();
                    buffer.set_pts(pts);
                    buffer.set_duration(duration);
                    buffer.set_offset(timestamp_data.offset);
                    buffer.set_offset_end(timestamp_data.offset + 1);
                    timestamp_data.offset = timestamp_data.offset + 1;
                    buffer.copy_from_slice(0, &vec).unwrap();
                }

                gst_debug!(self.cat, obj: element, "Produced buffer {:?}", buffer);
                Ok(buffer)
            }
        }
    }

    // This zero-sized struct is containing the static metadata of our element. It is only necessary to
    // be able to implement traits on it, but e.g. a plugin that registers multiple elements with the
    // same code would use this struct to store information about the concrete element. An example of
    // this would be a plugin that wraps around a library that has multiple decoders with the same API,
    // but wants (as it should) a separate element registered for each decoder.
    struct NdiAudioSrcStatic;

    // The basic trait for registering the type: This returns a name for the type and registers the
    // instance and class initializations functions with the type system, thus hooking everything
    // together.
    impl ImplTypeStatic<BaseSrc> for NdiAudioSrcStatic {
        fn get_name(&self) -> &str {
            "NdiAudioSrc"
        }

        fn new(&self, element: &BaseSrc) -> Box<BaseSrcImpl<BaseSrc>> {
            NdiAudioSrc::new(element)
        }

        fn class_init(&self, klass: &mut BaseSrcClass) {
            NdiAudioSrc::class_init(klass);
        }
    }

    // Registers the type for our element, and then registers in GStreamer under
    // the name NdiAudioSrc for being able to instantiate it via e.g.
    // gst::ElementFactory::make().
    pub fn register(plugin: &gst::Plugin) {
        let type_ = register_type(NdiAudioSrcStatic);
        gst::Element::register(plugin, "ndiaudiosrc", 0, type_);
    }
