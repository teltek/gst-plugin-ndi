#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]

use glib;
use gst;
use gst::prelude::*;
use gst_audio;
use gst_base::prelude::*;
use gst::Fraction;

use gst_plugin::base_src::*;
use gst_plugin::element::*;
use gst_plugin::object::*;
use gst_plugin::properties::*;

use std::sync::Mutex;
use std::{i32, u32};

use std::ptr;
use std::{thread, time};
use std::time::{SystemTime, UNIX_EPOCH};
use std::ffi::{CStr, CString};

use num_traits::float::Float;
use num_traits::cast::NumCast;
use byte_slice_cast::FromByteSlice;
use byte_slice_cast::AsSliceOf;

use ndilib::*;

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
    recv: Option<NdiInstance>,
    start_pts: Option<u64>,
}

impl Default for State {
    fn default() -> State {
        State {
            info: None,
            recv: None,
            start_pts: None,
        }
    }
}

struct ClockWait {
    clock_id: Option<gst::ClockId>,
    flushing: bool,
}
struct Pts{
    pts: u64,
    offset: u64,
}

// Struct containing all the element data
struct NdiAudioSrc {
    cat: gst::DebugCategory,
    settings: Mutex<Settings>,
    state: Mutex<State>,
    clock_wait: Mutex<ClockWait>,
    pts: Mutex<Pts>,
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
            clock_wait: Mutex::new(ClockWait {
                clock_id: None,
                flushing: true,
            }),
            pts: Mutex::new(Pts{
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


    fn process<F: Float + FromByteSlice>(
         data: &mut [u8],
         p_data: *const ::std::os::raw::c_float
     ){
         let data = data.as_mut_slice_of::<f64>().unwrap();
         // data = p_data;
         println!("asdf");
         unsafe{
         let v: Vec<f64> = Vec::from_raw_parts(p_data as *mut f64, 7372800, 7372800);
         // //let vec: &mut [F] = &v;
         // let a = v.as_slice();
         // *data = a.to_vec().as_slice();
     }
// ////////////*********************
//         use std::f64::consts::PI;
//
//         // Reinterpret our byte-slice as a slice containing elements of the type
//         // we're interested in. GStreamer requires for raw audio that the alignment
//         // of memory is correct, so this will never ever fail unless there is an
//         // actual bug elsewhere.
//         let data = data.as_mut_slice_of::<F>().unwrap();
//
//         // Convert all our parameters to the target type for calculations
//         //let vol: F = NumCast::from(vol).unwrap();
//         let freq = 440 as f64;
//         let rate = 48000 as f64;
//         let two_pi = 2.0 * PI;
//         let channels = 1;
//
//         // We're carrying a accumulator with up to 2pi around instead of working
//         // on the sample offset. High sample offsets cause too much inaccuracy when
//         // converted to floating point numbers and then iterated over in 1-steps
//         let mut accumulator = 0 as f64;
//         //let mut accumulator = *accumulator_ref;
//         let step = two_pi * freq / rate;
//
//         let mut vec: Vec<f64> = Vec::from_raw_parts(p_data as *mut f64, 7372800, 7372800);
//         data = vec.as_slice();
//         // for chunk in data.chunks_mut(channels as usize) {
//         //     // let value =  F::sin(NumCast::from(accumulator).unwrap());
//         //     // for sample in chunk {
//         //     //     *sample = value;
//         //     // }
//         //     //
//         //     // accumulator += step;
//         //     // if accumulator >= two_pi {
//         //     //     accumulator -= two_pi;
//         //     // }
//         //     chunk = p_data;
//         // }
//
//         //*accumulator_ref = accumulator;
// //////////////////*********************
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
                    gst_warning!(
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
                    gst_warning!(
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

    fn get_frame(ndisrc_struct: &NdiAudioSrc, element: &BaseSrc, pNDI_recv : NDIlib_recv_instance_t, pts2 : &mut u64, pts : &mut u64) -> NDIlib_audio_frame_v2_t{
        unsafe{
            let video_frame: NDIlib_video_frame_v2_t = Default::default();
            let audio_frame: NDIlib_audio_frame_v2_t = Default::default();
            let metadata_frame: NDIlib_metadata_frame_t = Default::default();

            //TODO Only create buffer when we got a video frame
            let mut frame = false;
            while !frame{
            let frame_type = NDIlib_recv_capture_v2(
                pNDI_recv,
                ptr::null(),
                &audio_frame,
                ptr::null(),
                1000,
            );
            match frame_type {
                NDIlib_frame_type_e::NDIlib_frame_type_video => {
                    println!("Videeeeeeo frrrame");
                    gst_debug!(ndisrc_struct.cat, obj: element, "Received video frame: {:?}", video_frame);
                    //frame = true;
                    //pts = ((video_frame.timestamp as u64) * 100) - state.start_pts.unwrap();
                    // println!("{:?}", pts/1000000);
                    *pts = ((video_frame.timestamp as u64) * 100);
                    if *pts2 == 0{
                        *pts2 = (video_frame.timestamp as u64) * 100;
                        *pts = 0;
                    }
                    else{
                        // println!("{:?}", video_frame.timecode * 100);
                        // println!("{:?}", pts2.pts);
                        *pts = (((video_frame.timestamp as u64) * 100) - *pts2);
                        //println!("{:?}", pts/1000000);
                    }

                }
                NDIlib_frame_type_e::NDIlib_frame_type_audio => {
                    gst_debug!(ndisrc_struct.cat, obj: element, "Received audio frame: {:?}", video_frame);
                    frame = true;
                    //pts = ((video_frame.timestamp as u64) * 100) - state.start_pts.unwrap();
                    // println!("{:?}", pts/1000000);
                    *pts = ((audio_frame.timestamp as u64) * 100);
                    if *pts2 == 0{
                        *pts2 = (audio_frame.timestamp as u64) * 100;
                        *pts = 0;
                    }
                    else{
                        // println!("{:?}", video_frame.timecode * 100);
                        // println!("{:?}", pts2.pts);
                        *pts = (((audio_frame.timestamp as u64) * 100) - *pts2);
                        //println!("{:?}", pts/1000000);
                    }
                }
                NDIlib_frame_type_e::NDIlib_frame_type_metadata => {
                    // println!(
                    //     "Tengo metadata {} '{}'",
                    //     metadata_frame.length,
                    //     CStr::from_ptr(metadata_frame.p_data)
                    //     .to_string_lossy()
                    //     .into_owned(),
                    // );
                    //TODO Change gst_warning to gst_debug
                    gst_debug!(ndisrc_struct.cat, obj: element, "Received metadata frame: {:?}", CStr::from_ptr(metadata_frame.p_data).to_string_lossy().into_owned(),);
                }
                NDIlib_frame_type_e::NDIlib_frame_type_error => {
                    // println!(
                    //     "Tengo error {} '{}'",
                    //     metadata_frame.length,
                    //     CStr::from_ptr(metadata_frame.p_data)
                    //     .to_string_lossy()
                    //     .into_owned(),
                    // );
                    //TODO Change gst_warning to gst_debug
                    gst_debug!(ndisrc_struct.cat, obj: element, "Received error frame: {:?}", CStr::from_ptr(metadata_frame.p_data).to_string_lossy().into_owned());
                    // break;
                }
                _ => println!("Tengo {:?}", frame_type),
            }
             }
            return audio_frame;
        }
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
            self.unlock_stop(element);

            gst_warning!(self.cat, obj: element, "Starting");
            let mut state = self.state.lock().unwrap();
            //let mut settings = self.settings.lock().unwrap();
            let settings = self.settings.lock().unwrap();

            //let mut pNDI_recv = state.recv;
            unsafe {
                if !NDIlib_initialize() {
                    //println!("Cannot run NDI: NDIlib_initialize error.");
                    gst_element_error!(element, gst::CoreError::Negotiation, ["Cannot run NDI: NDIlib_initialize error"]);
                    return false;
                }

                let mut source: NDIlib_source_t = NDIlib_source_t{p_ndi_name: ptr::null(),
                    p_ip_address: ptr::null()};

                    // print!("{:?}", settings.stream_name);
                    // print!("{:?}", settings.ip);

                    //TODO default values
                    let NDI_find_create_desc: NDIlib_find_create_t = Default::default();
                    let pNDI_find = NDIlib_find_create_v2(&NDI_find_create_desc);
                    let ip_ptr = CString::new(settings.ip.clone()).unwrap();
                    if (ip_ptr == CString::new("").unwrap()){
                        if pNDI_find.is_null() {
                            //println!("Cannot run NDI: NDIlib_find_create_v2 error.");
                            gst_element_error!(element, gst::CoreError::Negotiation, ["Cannot run NDI: NDIlib_find_create_v2 error"]);
                            return false;
                        }

                        let mut total_sources: u32 = 0;
                        let mut p_sources = ptr::null();
                        //TODO Delete while. If not, will loop until a source it's available
                        //while total_sources == 0 {
                        // TODO Sleep 1s to wait for all sources
                        thread::sleep(time::Duration::from_millis(2000));
                        p_sources = NDIlib_find_get_current_sources(pNDI_find, &mut total_sources as *mut u32);
                        //}

                        // We need at least one source
                        if p_sources.is_null() {
                            //println!("Error getting NDIlib_find_get_current_sources.");
                            gst_element_error!(element, gst::CoreError::Negotiation, ["Error getting NDIlib_find_get_current_sources"]);
                            return false;
                            //::std::process::exit(1);
                        }

                        let mut no_source: isize = -1;
                        for i in 0..total_sources as isize{
                            if CStr::from_ptr((*p_sources.offset(i)).p_ndi_name)
                            .to_string_lossy()
                            .into_owned() == settings.stream_name{
                                no_source = i;
                                break;
                            }
                        }
                        if no_source  == -1 {
                            gst_element_error!(element, gst::CoreError::Negotiation, ["Stream name not found"]);
                            return false;
                        }
                        println!(
                            "Total_sources {}: Name '{}' Address '{}'",
                            total_sources,
                            CStr::from_ptr((*p_sources.offset(no_source)).p_ndi_name)
                            .to_string_lossy()
                            .into_owned(),
                            CStr::from_ptr((*p_sources.offset(no_source)).p_ip_address)
                            .to_string_lossy()
                            .into_owned()
                        );
                        source = *p_sources.offset(no_source).clone();
                    }
                    else{
                        source.p_ip_address = ip_ptr.as_ptr();
                        println!(
                            "Address '{}'",
                            CStr::from_ptr(source.p_ip_address)
                            .to_string_lossy()
                            .into_owned()
                        );
                    }

                    // We now have at least one source, so we create a receiver to look at it.
                    // We tell it that we prefer YCbCr video since it is more efficient for us. If the source has an alpha channel
                    // it will still be provided in BGRA
                    let p_ndi_name = CString::new("Galicaster NDI Receiver").unwrap();
                    let NDI_recv_create_desc = NDIlib_recv_create_v3_t {
                        source_to_connect_to: source,
                        p_ndi_name: p_ndi_name.as_ptr(),
                        ..Default::default()
                    };

                    let pNDI_recv = NDIlib_recv_create_v3(&NDI_recv_create_desc);
                    if pNDI_recv.is_null() {
                        //println!("Cannot run NDI: NDIlib_recv_create_v3 error.");
                        gst_element_error!(element, gst::CoreError::Negotiation, ["Cannot run NDI: NDIlib_recv_create_v3 error"]);
                        return false;
                        //::std::process::exit(1);
                    }

                    // Destroy the NDI finder. We needed to have access to the pointers to p_sources[0]
                    NDIlib_find_destroy(pNDI_find);

                    // We are now going to mark this source as being on program output for tally purposes (but not on preview)
                    let tally_state: NDIlib_tally_t = Default::default();
                    NDIlib_recv_set_tally(pNDI_recv, &tally_state);

                    // Enable Hardwqre Decompression support if this support has it. Please read the caveats in the documentation
                    // regarding this. There are times in which it might reduce the performance although on small stream numbers
                    // it almost always yields the same or better performance.
                    let data = CString::new("<ndi_hwaccel enabled=\"true\"/>").unwrap();
                    let enable_hw_accel = NDIlib_metadata_frame_t {
                        length: data.to_bytes().len() as i32,
                        timecode: 0,
                        p_data: data.as_ptr(),
                    };

                    NDIlib_recv_send_metadata(pNDI_recv, &enable_hw_accel);
                    state.recv = Some(NdiInstance{recv: pNDI_recv});
                    let start = SystemTime::now();
                    let since_the_epoch = start.duration_since(UNIX_EPOCH)
                    .expect("Time went backwards");
                    println!("{:?}", since_the_epoch);
                    state.start_pts = Some(since_the_epoch.as_secs() * 1000000000 +
                    since_the_epoch.subsec_nanos() as u64);
                    //TODO Another way to save NDI_recv variable
                    // *state = State{
                    //     info: state.info.clone(),
                    //     recv: Some(NdiInstance{recv: pNDI_recv}),
                    // };
                }

                true
            }

            // Called when shutting down the element so we can release all stream-related state
            fn stop(&self, element: &BaseSrc) -> bool {
                // Reset state
                let state = self.state.lock().unwrap();
                let recv = match state.recv{
                    None => {
                        //println!("pNDI_recv no encontrado");
                        gst_element_error!(element, gst::CoreError::Negotiation, ["No encontramos ndi recv"]);
                        return true;
                    }
                    Some(ref recv) => recv.clone(),
                };
                let pNDI_recv = recv.recv;
                unsafe{
                    NDIlib_recv_destroy(pNDI_recv);
                    //NDIlib_destroy();
                }
                // Commented because when adding ndi destroy stopped in this line
                //*self.state.lock().unwrap() = Default::default();
                self.unlock(element);
                gst_info!(self.cat, obj: element, "Stopped");

                true
            }

            fn fixate(&self, element: &BaseSrc, caps: gst::Caps) -> gst::Caps {
                //We need to set the correct caps resolution and framerate
                let state = self.state.lock().unwrap();
                let recv = match state.recv{
                    None => {
                        //TODO Update gst_element_error with one more descriptive
                        //println!("pNDI_recv no encontrado");
                        gst_element_error!(element, gst::CoreError::Negotiation, ["No encontramos ndi recv"]);
                        //TODO if none not return anything
                        return caps;
                    }
                    Some(ref recv) => recv.clone(),
                };

                let pNDI_recv = recv.recv;
                let mut pts2 = self.pts.lock().unwrap();
                let mut pts: u64 = 0;

                let audio_frame: NDIlib_audio_frame_v2_t = get_frame(self, element, pNDI_recv, &mut pts2.pts, &mut pts);
                let mut caps = gst::Caps::truncate(caps);
                {
                    let caps = caps.make_mut();
                    let s = caps.get_mut_structure(0).unwrap();
                    s.fixate_field_nearest_int("rate", audio_frame.sample_rate);
                    s.fixate_field_nearest_int("channels", audio_frame.no_channels);
                    //s.fixate_field_nearest_fraction("framerate", Fraction::new(video_frame.frame_rate_N, video_frame.frame_rate_D));
                    //s.fixate_field_str("format", &gst_video::VideoFormat::Rgb.to_string());
                    //caps.set_simple(&[("width", &(1600 as i32))]);
                    //s.set_value("width", &(1600 as i32));
                }

                // Let BaseSrc fixate anything else for us. We could've alternatively have
                // called Caps::fixate() here
                element.parent_fixate(caps)
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

                let mut pts2 = self.pts.lock().unwrap();
                // Get a locked reference to our state, i.e. the input and output AudioInfo
                let state = self.state.lock().unwrap();
                let _info = match state.info {
                    None => {
                        gst_element_error!(element, gst::CoreError::Negotiation, ["Have no caps yet"]);
                        return Err(gst::FlowReturn::NotNegotiated);
                    }
                    Some(ref info) => info.clone(),
                };
                //let mut pNDI_recva = ptr::null();
                // {
                let recv = match state.recv{
                    None => {
                        //TODO Update gst_element_error with one more descriptive
                        //println!("pNDI_recv no encontrado");
                        gst_element_error!(element, gst::CoreError::Negotiation, ["No encontramos ndi recv"]);
                        return Err(gst::FlowReturn::NotNegotiated);
                    }
                    Some(ref recv) => recv.clone(),
                };
                let pNDI_recv = recv.recv;
                // }

                let start_pts = match state.start_pts {
                    None => {
                        gst_element_error!(element, gst::CoreError::Negotiation, ["Have no caps yet"]);
                        return Err(gst::FlowReturn::NotNegotiated);
                    }
                    Some(ref start_pts) => start_pts.clone(),
                };

                unsafe{
                    // // loop {
                    let mut pts: u64 = 0;
                    let video_frame: NDIlib_video_frame_v2_t = Default::default();
                    let audio_frame: NDIlib_audio_frame_v2_t = get_frame(self, element, pNDI_recv, &mut pts2.pts, &mut pts);
                    let metadata_frame: NDIlib_metadata_frame_t = Default::default();

                    let buff_size = (audio_frame.no_channels * audio_frame.no_samples) as usize;
                    //let buff_size = 126864 as usize;
                    //let buff_size = 7372800 as usize;
                    println!("1");
                    let mut audio_frame_16s: NDIlib_audio_frame_interleaved_16s_t = Default::default();
                    let thing: [::std::os::raw::c_short; 0] = [];
                    let a : *const i16 = &thing;
                    audio_frame_16s.p_data = a;
                    NDIlib_util_audio_to_interleaved_16s_v2(&audio_frame, &audio_frame_16s);
                    println!("2");
                    println!("{:?}", audio_frame_16s);
                    let mut buffer = gst::Buffer::with_size(buff_size).unwrap();
                    {
                        let  vec = Vec::from_raw_parts(audio_frame_16s.p_data as *mut u8, buff_size, buff_size);
                        //TODO Set pts, duration and other info about the buffer
                        let pts: gst::ClockTime = (pts).into();
                        let duration: gst::ClockTime = (40000000).into();
                        let buffer = buffer.get_mut().unwrap();
                        buffer.set_pts(pts);
                        buffer.set_duration(duration);
                        buffer.set_offset(pts2.offset);
                        buffer.set_offset_end(pts2.offset + 1);
                        pts2.offset = pts2.offset +1;
                        println!("{:?}", buff_size);
                        //println!("{:?}", vec);

                        // let mut vec: Vec<f64> = Vec::from_raw_parts(audio_frame.p_data as *mut f64, 7372800, 7372800);
                        //
                        // println!("aasdfasdf");
                        // print
                        buffer.copy_from_slice(0, &vec).unwrap();
                        // let mut map = buffer.map_writable().unwrap();
                        // let data = map.as_mut_slice();
                        //
                        // let mut data = data.as_mut_slice_of::<f64>().unwrap();
                        // data = vec.as_mut_slice();


                        // Self::process::<f64>(
                        //     data,
                        //     audio_frame.p_data,
                        // );
                    }

                    gst_debug!(self.cat, obj: element, "Produced buffer {:?}", buffer);
                    Ok(buffer)
                }
            }


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
