#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]

// Copyright (C) 2017 Sebastian Dröge <sebastian@centricular.com>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate glib;
#[macro_use]
extern crate gst_plugin;
#[macro_use]
extern crate gstreamer as gst;
extern crate gstreamer_audio as gst_audio;
extern crate gstreamer_base as gst_base;
extern crate gstreamer_video as gst_video;

extern crate byte_slice_cast;
extern crate num_traits;
#[macro_use]
extern crate lazy_static;

mod ndivideosrc;
mod ndiaudiosrc;
pub mod ndilib;

use std::{thread, time};
//use std::time::{SystemTime, UNIX_EPOCH};
use std::ffi::{CStr, CString};
use ndilib::*;
use gst_plugin::base_src::*;

use std::collections::HashMap;
use std::sync::Mutex;

use gst::GstObjectExt;

// Plugin entry point that should register all elements provided by this plugin,
// and everything else that this plugin might provide (e.g. typefinders or device providers).
fn plugin_init(plugin: &gst::Plugin) -> bool {
    ndivideosrc::register(plugin);
    ndiaudiosrc::register(plugin);
    true
}

struct ndi_receiver_info{
    stream_name: String,
    ip: String,
    video: bool,
    audio: bool,
    ndi_instance: NdiInstance,
    id: i8,
}

struct Ndi{
    initial_timestamp: u64,
    start_pts: gst::ClockTime,
}

static mut ndi_struct: Ndi = Ndi{
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

fn connect_ndi(cat: gst::DebugCategory , element: &BaseSrc,  ip: String,  stream_name: String) -> i8{
    unsafe {
        gst_debug!(cat, obj: element, "Starting NDI connection...");

        let mut receivers = hashmap_receivers.lock().unwrap();
        let mut audio = false;
        let mut video = false;
        id_receiver += 1;

        //FIXME Search for another way to know if the source is an audio or a video source
        if element.get_name().contains("audiosrc"){
            audio = true;
        }
        else
        {
            video = true;
        }

        for val in receivers.values_mut(){
            if val.ip == ip || val.stream_name == stream_name{
                if (val.audio && val.video) || (val.audio && audio) || (val.video && video){
                    break;
                }
                else {
                    if video {
                        val.video = video;
                    }
                    else{
                        val.audio = audio;
                    }
                    return val.id;
                }
            }

        }

        if !NDIlib_initialize() {
            gst_element_error!(element, gst::CoreError::Negotiation, ["Cannot run NDI: NDIlib_initialize error"]);
            // return false;
            return 0;
        }

        //TODO default values
        let NDI_find_create_desc: NDIlib_find_create_t = Default::default();
        let pNDI_find = NDIlib_find_create_v2(&NDI_find_create_desc);
        //let ip_ptr = CString::new(ip.clone()).unwrap();
        if pNDI_find.is_null() {
            gst_element_error!(element, gst::CoreError::Negotiation, ["Cannot run NDI: NDIlib_find_create_v2 error"]);
            // return false;
            return 0;
        }

        let mut total_sources: u32 = 0;
        let p_sources;

        // TODO Sleep 1s to wait for all sources
        thread::sleep(time::Duration::from_millis(2000));
        p_sources = NDIlib_find_get_current_sources(pNDI_find, &mut total_sources as *mut u32);

        // We need at least one source
        if p_sources.is_null() {
            gst_element_error!(element, gst::CoreError::Negotiation, ["Error getting NDIlib_find_get_current_sources"]);
            // return false;
            return 0;
        }

        let mut no_source: isize = -1;
        for i in 0..total_sources as isize{
            if CStr::from_ptr((*p_sources.offset(i)).p_ndi_name).to_string_lossy().into_owned() == stream_name ||
            CStr::from_ptr((*p_sources.offset(i)).p_ip_address).to_string_lossy().into_owned() == ip{
                no_source = i;
                break;
            }
        }
        if no_source  == -1 {
            gst_element_error!(element, gst::ResourceError::OpenRead, ["Stream not found"]);
            // return false;
            return 0;
        }

        gst_debug!(cat, obj: element, "Total sources in network {}: Connecting to NDI source with name '{}' and address '{}'", total_sources,
        CStr::from_ptr((*p_sources.offset(no_source)).p_ndi_name)
        .to_string_lossy()
        .into_owned(),
        CStr::from_ptr((*p_sources.offset(no_source)).p_ip_address)
        .to_string_lossy()
        .into_owned());

        let source = *p_sources.offset(no_source).clone();

        let source_ip = CStr::from_ptr(source.p_ip_address).to_string_lossy().into_owned();
        let source_name = CStr::from_ptr(source.p_ndi_name).to_string_lossy().into_owned();

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
            // return false;
            return 0;
        }

        // Destroy the NDI finder. We needed to have access to the pointers to p_sources[0]
        NDIlib_find_destroy(pNDI_find);

        // We are now going to mark this source as being on program output for tally purposes (but not on preview)
        let tally_state: NDIlib_tally_t = Default::default();
        NDIlib_recv_set_tally(pNDI_recv, &tally_state);

        // Enable Hardware Decompression support if this support has it. Please read the caveats in the documentation
        // regarding this. There are times in which it might reduce the performance although on small stream numbers
        // it almost always yields the same or better performance.
        let data = CString::new("<ndi_hwaccel enabled=\"true\"/>").unwrap();
        let enable_hw_accel = NDIlib_metadata_frame_t {
            length: data.to_bytes().len() as i32,
            timecode: 0,
            p_data: data.as_ptr(),
        };

        NDIlib_recv_send_metadata(pNDI_recv, &enable_hw_accel);

        receivers.insert(id_receiver, ndi_receiver_info{stream_name: source_name.clone(), ip: source_ip.clone(), video:video, audio: audio, ndi_instance: NdiInstance{recv: pNDI_recv}, id: id_receiver});

        // let start = SystemTime::now();
        // let since_the_epoch = start.duration_since(UNIX_EPOCH)
        // .expect("Time went backwards");
        // println!("{:?}", since_the_epoch);
        // ndi_struct.start_pts = Some(since_the_epoch.as_secs() * 1000000000 +
        // since_the_epoch.subsec_nanos() as u64);
        gst_debug!(cat, obj: element, "Started NDI connection");
        return id_receiver;
    }
}

fn stop_ndi(cat: gst::DebugCategory , element: &BaseSrc, id: i8) -> bool{
    gst_debug!(cat, obj: element, "Closing NDI connection...");
    unsafe{
        let mut receivers = hashmap_receivers.lock().unwrap();
        {
            let val = receivers.get_mut(&id).unwrap();
            if val.video && val.audio{
                if element.get_name().contains("audiosrc"){
                    val.audio = false;
                }
                else{
                    val.video = false;
                }
                return true;
            }

            let recv = &val.ndi_instance;
            let pNDI_recv = recv.recv;
            NDIlib_recv_destroy(pNDI_recv);
            // ndi_struct.recv = None;
            NDIlib_destroy();
        }
        receivers.remove(&id);
        gst_debug!(cat, obj: element, "Closed NDI connection");
        return true;
    }
}

// Static plugin metdata that is directly stored in the plugin shared object and read by GStreamer
// upon loading.
// Plugin name, plugin description, plugin entry point function, version number of this plugin,
// license of the plugin, source package name, binary package name, origin where it comes from
// and the date/time of release.
plugin_define!(
    b"ndi\0",
    b"NewTek NDI Plugin\0",
    plugin_init,
    b"1.0\0",
    b"MIT/X11\0",
    b"ndi\0",
    b"ndi\0",
    b"https://gitlab.teltek.es/rubenrua/ndi-rs.git\0",
    b"2018-04-09\0"
);
