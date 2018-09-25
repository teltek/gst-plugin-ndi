#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]

extern crate glib;
extern crate gobject_subclass;

#[macro_use]
extern crate gst_plugin;
#[macro_use]
extern crate gstreamer as gst;
use gst::prelude::*;
extern crate gstreamer_audio as gst_audio;
extern crate gstreamer_base as gst_base;
extern crate gstreamer_video as gst_video;

#[macro_use]
extern crate lazy_static;

mod ndiaudiosrc;
pub mod ndisys;
mod ndivideosrc;

use gst_plugin::base_src::*;
use ndisys::*;
use std::ffi::{CStr, CString};
use std::{thread, time};

use std::collections::HashMap;
use std::sync::Mutex;

use gst::GstObjectExt;

fn plugin_init(plugin: &gst::Plugin) -> bool {
    ndivideosrc::register(plugin);
    ndiaudiosrc::register(plugin);
    true
}

struct ndi_receiver_info {
    stream_name: String,
    ip: String,
    video: bool,
    audio: bool,
    ndi_instance: NdiInstance,
    id: i8,
}

struct Ndi {
    initial_timestamp: u64,
    start_pts: gst::ClockTime,
}

static mut ndi_struct: Ndi = Ndi {
    initial_timestamp: 0,
    start_pts: gst::ClockTime(Some(0)),
};

lazy_static! {
    static ref hashmap_receivers: Mutex<HashMap<i8, ndi_receiver_info>> = {
        let m = HashMap::new();
        Mutex::new(m)
    };
}

static mut id_receiver: i8 = 0;

fn connect_ndi(cat: gst::DebugCategory, element: &BaseSrc, ip: &str, stream_name: &str) -> i8 {
    gst_debug!(cat, obj: element, "Starting NDI connection...");

    let mut receivers = hashmap_receivers.lock().unwrap();
    let mut audio = false;
    let mut video = false;

    if element.get_factory().map(|f| f.get_name() == "ndiaudiosrc").unwrap_or(false) {
        audio = true;
    } else {
        video = true;
    }

    for val in receivers.values_mut() {
        if val.ip == ip || val.stream_name == stream_name {
            if (val.audio && val.video) || (val.audio && audio) || (val.video && video) {
                continue;
            } else {
                if video {
                    val.video = video;
                } else {
                    val.audio = audio;
                }
                return val.id;
            }
        }
    }
    unsafe {
        if !NDIlib_initialize() {
            gst_element_error!(
                element,
                gst::CoreError::Negotiation,
                ["Cannot run NDI: NDIlib_initialize error"]
            );
            return 0;
        }

        let NDI_find_create_desc: NDIlib_find_create_t = Default::default();
        let pNDI_find = NDIlib_find_create_v2(&NDI_find_create_desc);
        if pNDI_find.is_null() {
            gst_element_error!(
                element,
                gst::CoreError::Negotiation,
                ["Cannot run NDI: NDIlib_find_create_v2 error"]
            );
            return 0;
        }

        let mut total_sources: u32 = 0;
        let p_sources;

        // TODO Sleep 1s to wait for all sources
        thread::sleep(time::Duration::from_millis(2000));
        p_sources = NDIlib_find_get_current_sources(pNDI_find, &mut total_sources as *mut u32);

        // We need at least one source
        if p_sources.is_null() {
            gst_element_error!(
                element,
                gst::CoreError::Negotiation,
                ["Error getting NDIlib_find_get_current_sources"]
            );
            return 0;
        }

        let mut no_source: isize = -1;
        for i in 0..total_sources as isize {
            if CStr::from_ptr((*p_sources.offset(i)).p_ndi_name)
                .to_string_lossy()
                .into_owned()
                == stream_name
                || CStr::from_ptr((*p_sources.offset(i)).p_ip_address)
                    .to_string_lossy()
                    .into_owned()
                    == ip
            {
                no_source = i;
                break;
            }
        }
        if no_source == -1 {
            gst_element_error!(element, gst::ResourceError::OpenRead, ["Stream not found"]);
            return 0;
        }

        gst_debug!(
            cat,
            obj: element,
            "Total sources in network {}: Connecting to NDI source with name '{}' and address '{}'",
            total_sources,
            CStr::from_ptr((*p_sources.offset(no_source)).p_ndi_name)
                .to_string_lossy()
                .into_owned(),
            CStr::from_ptr((*p_sources.offset(no_source)).p_ip_address)
                .to_string_lossy()
                .into_owned()
        );

        let source = *p_sources.offset(no_source);

        let source_ip = CStr::from_ptr(source.p_ip_address)
            .to_string_lossy()
            .into_owned();
        let source_name = CStr::from_ptr(source.p_ndi_name)
            .to_string_lossy()
            .into_owned();

        let p_ndi_name = CString::new("Galicaster NDI Receiver").unwrap();
        let NDI_recv_create_desc = NDIlib_recv_create_v3_t {
            source_to_connect_to: source,
            p_ndi_name: p_ndi_name.as_ptr(),
            ..Default::default()
        };

        let pNDI_recv = NDIlib_recv_create_v3(&NDI_recv_create_desc);
        if pNDI_recv.is_null() {
            gst_element_error!(
                element,
                gst::CoreError::Negotiation,
                ["Cannot run NDI: NDIlib_recv_create_v3 error"]
            );
            return 0;
        }

        NDIlib_find_destroy(pNDI_find);

        let tally_state: NDIlib_tally_t = Default::default();
        NDIlib_recv_set_tally(pNDI_recv, &tally_state);

        let data = CString::new("<ndi_hwaccel enabled=\"true\"/>").unwrap();
        let enable_hw_accel = NDIlib_metadata_frame_t {
            length: data.to_bytes().len() as i32,
            timecode: 0,
            p_data: data.as_ptr(),
        };

        NDIlib_recv_send_metadata(pNDI_recv, &enable_hw_accel);

        id_receiver += 1;
        receivers.insert(
            id_receiver,
            ndi_receiver_info {
                stream_name: source_name.clone(),
                ip: source_ip.clone(),
                video,
                audio,
                ndi_instance: NdiInstance { recv: pNDI_recv },
                id: id_receiver,
            },
        );

        gst_debug!(cat, obj: element, "Started NDI connection");
        id_receiver
    }
}

fn stop_ndi(cat: gst::DebugCategory, element: &BaseSrc, id: i8) -> bool {
    gst_debug!(cat, obj: element, "Closing NDI connection...");
    let mut receivers = hashmap_receivers.lock().unwrap();
    {
        let val = receivers.get_mut(&id).unwrap();
        if val.video && val.audio {
            if element.get_name().contains("audiosrc") {
                val.audio = false;
            } else {
                val.video = false;
            }
            return true;
        }

        let recv = &val.ndi_instance;
        let pNDI_recv = recv.recv;
        unsafe {
            NDIlib_recv_destroy(pNDI_recv);
            // ndi_struct.recv = None;
            NDIlib_destroy();
        }
    }
    receivers.remove(&id);
    gst_debug!(cat, obj: element, "Closed NDI connection");
    true
}

plugin_define!(
    b"ndi\0",
    b"NewTek NDI Plugin\0",
    plugin_init,
    b"1.0.0\0",
    b"LGPL\0",
    b"ndi\0",
    b"ndi\0",
    b"https://github.com/teltek/gst-plugin-ndi\0",
    b"2018-04-09\0"
);
