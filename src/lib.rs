#[macro_use]
extern crate glib;
use glib::prelude::*;
use glib::subclass::prelude::*;
#[macro_use]
extern crate gstreamer as gst;
extern crate gstreamer_audio as gst_audio;
extern crate gstreamer_base as gst_base;
extern crate gstreamer_video as gst_video;

#[macro_use]
extern crate lazy_static;
extern crate byte_slice_cast;

pub mod ndisys;
pub mod ndi;
mod ndiaudiosrc;
mod ndivideosrc;

use ndisys::*;
use ndi::*;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    if !ndi::initialize() {
        return Err(glib_bool_error!("Cannot initialize NDI"));
    }

    ndivideosrc::register(plugin)?;
    ndiaudiosrc::register(plugin)?;
    Ok(())
}

struct ReceiverInfo {
    id: usize,
    stream_name: String,
    ip: String,
    video: bool,
    audio: bool,
    ndi_instance: RecvInstance,
}

lazy_static! {
    static ref HASHMAP_RECEIVERS: Mutex<HashMap<usize, ReceiverInfo>> = {
        let m = HashMap::new();
        Mutex::new(m)
    };
}

static ID_RECEIVER: AtomicUsize = AtomicUsize::new(0);

fn connect_ndi(
    cat: gst::DebugCategory,
    element: &gst_base::BaseSrc,
    ip: &str,
    stream_name: &str,
) -> Option<usize> {
    gst_debug!(cat, obj: element, "Starting NDI connection...");

    let mut receivers = HASHMAP_RECEIVERS.lock().unwrap();

    let video = element.get_type() == ndivideosrc::NdiVideoSrc::get_type();

    for val in receivers.values_mut() {
        if val.ip == ip || val.stream_name == stream_name {
            if (val.audio && val.video) || (val.audio && !video) || (val.video && video) {
                continue;
            } else {
                if video {
                    val.video = video;
                } else {
                    val.audio = !video;
                }
                return Some(val.id);
            }
        }
    }

    let mut find = match FindInstance::builder().build() {
        None => {
            gst_element_error!(
                element,
                gst::CoreError::Negotiation,
                ["Cannot run NDI: NDIlib_find_create_v2 error"]
            );
            return None;
        },
        Some(find) => find,
    };

    // TODO Sleep 1s to wait for all sources
    find.wait_for_sources(2000);

    let sources = find.get_current_sources();

    // We need at least one source
    if sources.is_empty() {
        gst_element_error!(
            element,
            gst::CoreError::Negotiation,
            ["Error getting NDIlib_find_get_current_sources"]
        );
        return None;
    }

    let source = sources.iter().find(|s| {
        s.ndi_name() == stream_name || s.ip_address() == ip
    });

    let source = match source {
        None => {
            gst_element_error!(element, gst::ResourceError::OpenRead, ["Stream not found"]);
            return None;
        },
        Some(source) => source,
    };

    gst_debug!(
        cat,
        obj: element,
        "Total sources in network {}: Connecting to NDI source with name '{}' and address '{}'",
        sources.len(),
        source.ndi_name(),
        source.ip_address(),
    );

    let recv = RecvInstance::builder(&source, "Galicaster NDI Receiver")
        .bandwidth(NDIlib_recv_bandwidth_e::NDIlib_recv_bandwidth_highest)
        .color_format(NDIlib_recv_color_format_e::NDIlib_recv_color_format_UYVY_BGRA)
        .allow_video_fields(true)
        .build();
    let recv = match recv {
        None => {
            gst_element_error!(
                element,
                gst::CoreError::Negotiation,
                ["Cannot run NDI: NDIlib_recv_create_v3 error"]
            );
            return None;
        },
        Some(recv) => recv,
    };

    recv.set_tally(&Tally::default());

    let enable_hw_accel = MetadataFrame::new(0, Some("<ndi_hwaccel enabled=\"true\"/>"));
    recv.send_metadata(&enable_hw_accel);

    let id_receiver = ID_RECEIVER.fetch_add(1, Ordering::SeqCst);
    receivers.insert(
        id_receiver,
        ReceiverInfo {
            stream_name: source.ndi_name().to_owned(),
            ip: source.ip_address().to_owned(),
            video,
            audio: !video,
            ndi_instance: recv,
            id: id_receiver,
        },
    );

    gst_debug!(cat, obj: element, "Started NDI connection");
    Some(id_receiver)
}

fn stop_ndi(cat: gst::DebugCategory, element: &gst_base::BaseSrc, id: usize) -> bool {
    gst_debug!(cat, obj: element, "Closing NDI connection...");
    let mut receivers = HASHMAP_RECEIVERS.lock().unwrap();
    {
        let val = receivers.get_mut(&id).unwrap();
        if val.video && val.audio {
            let video = element.get_type() == ndivideosrc::NdiVideoSrc::get_type();
            if video {
                val.video = false;
            } else {
                val.audio = false;
            }
            return true;
        }
    }
    receivers.remove(&id);
    gst_debug!(cat, obj: element, "Closed NDI connection");
    true
}

gst_plugin_define!(
    ndi,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    concat!(env!("CARGO_PKG_VERSION"), "-", env!("COMMIT_ID")),
    "LGPL",
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_REPOSITORY"),
    env!("BUILD_REL_DATE")
);
