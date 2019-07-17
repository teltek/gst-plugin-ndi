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

pub mod ndi;
mod ndiaudiosrc;
pub mod ndisys;
mod ndivideosrc;

use ndi::*;
use ndisys::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
#[repr(u32)]
pub enum TimestampMode {
    ReceiveTime = 0,
    Timecode = 1,
    Timestamp = 2,
}

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
    ndi_name: String,
    ip_address: String,
    video: bool,
    audio: bool,
    ndi_instance: RecvInstance,
}

lazy_static! {
    static ref HASHMAP_RECEIVERS: Mutex<HashMap<usize, ReceiverInfo>> = {
        let m = HashMap::new();
        Mutex::new(m)
    };

    static ref DEFAULT_RECEIVER_NDI_NAME: String = {
        format!("GStreamer NDI Source {}-{}", env!("CARGO_PKG_VERSION"), env!("COMMIT_ID"))
    };

    #[cfg(feature = "reference-timestamps")]
    static ref TIMECODE_CAPS: gst::Caps = {
        gst::Caps::new_simple("timestamp/x-ndi-timecode", &[])
    };

    #[cfg(feature = "reference-timestamps")]
    static ref TIMESTAMP_CAPS: gst::Caps = {
        gst::Caps::new_simple("timestamp/x-ndi-timestamp", &[])
    };
}

static ID_RECEIVER: AtomicUsize = AtomicUsize::new(0);

fn connect_ndi(
    cat: gst::DebugCategory,
    element: &gst_base::BaseSrc,
    ip_address: Option<&str>,
    ndi_name: Option<&str>,
    receiver_ndi_name: &str,
    connect_timeout: u32,
    bandwidth: NDIlib_recv_bandwidth_e,
) -> Option<usize> {
    gst_debug!(cat, obj: element, "Starting NDI connection...");

    let mut receivers = HASHMAP_RECEIVERS.lock().unwrap();

    let video = element.get_type() == ndivideosrc::NdiVideoSrc::get_type();

    for val in receivers.values_mut() {
        if Some(val.ip_address.as_str()) == ip_address || Some(val.ndi_name.as_str()) == ndi_name {
            if (val.video || !video) && (val.audio || video) {
                continue;
            } else {
                if video {
                    val.video = true;
                } else {
                    val.audio = true;
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
        }
        Some(find) => find,
    };

    let timeout = time::Instant::now();
    let source = loop {
        find.wait_for_sources(50);

        let sources = find.get_current_sources();

        gst_debug!(
            cat,
            obj: element,
            "Total sources found in network {}",
            sources.len(),
        );

        let source = sources
            .iter()
            .find(|s| Some(s.ndi_name()) == ndi_name || Some(s.ip_address()) == ip_address);

        if let Some(source) = source {
            break source.to_owned();
        }

        if timeout.elapsed().as_millis() >= connect_timeout as u128 {
            gst_element_error!(element, gst::ResourceError::NotFound, ["Stream not found"]);
            return None;
        }
    };

    gst_debug!(
        cat,
        obj: element,
        "Connecting to NDI source with ndi-name '{}' and ip-address '{}'",
        source.ndi_name(),
        source.ip_address(),
    );

    // FIXME: Ideally we would use NDIlib_recv_color_format_fastest here but that seems to be
    // broken with interlaced content currently
    let recv = RecvInstance::builder(&source, receiver_ndi_name)
        .bandwidth(bandwidth)
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
        }
        Some(recv) => recv,
    };

    recv.set_tally(&Tally::default());

    let enable_hw_accel = MetadataFrame::new(0, Some("<ndi_hwaccel enabled=\"true\"/>"));
    recv.send_metadata(&enable_hw_accel);

    let id_receiver = ID_RECEIVER.fetch_add(1, Ordering::SeqCst);
    receivers.insert(
        id_receiver,
        ReceiverInfo {
            id: id_receiver,
            ndi_name: source.ndi_name().to_owned(),
            ip_address: source.ip_address().to_owned(),
            video,
            audio: !video,
            ndi_instance: recv,
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

impl glib::translate::ToGlib for TimestampMode {
    type GlibType = i32;

    fn to_glib(&self) -> i32 {
        *self as i32
    }
}

impl glib::translate::FromGlib<i32> for TimestampMode {
    fn from_glib(value: i32) -> Self {
        match value {
            0 => TimestampMode::ReceiveTime,
            1 => TimestampMode::Timecode,
            2 => TimestampMode::Timestamp,
            _ => unreachable!(),
        }
    }
}

impl StaticType for TimestampMode {
    fn static_type() -> glib::Type {
        timestamp_mode_get_type()
    }
}

impl<'a> glib::value::FromValueOptional<'a> for TimestampMode {
    unsafe fn from_value_optional(value: &glib::Value) -> Option<Self> {
        Some(glib::value::FromValue::from_value(value))
    }
}

impl<'a> glib::value::FromValue<'a> for TimestampMode {
    unsafe fn from_value(value: &glib::Value) -> Self {
        use glib::translate::ToGlibPtr;

        glib::translate::from_glib(gobject_sys::g_value_get_enum(value.to_glib_none().0))
    }
}

impl glib::value::SetValue for TimestampMode {
    unsafe fn set_value(value: &mut glib::Value, this: &Self) {
        use glib::translate::{ToGlib, ToGlibPtrMut};

        gobject_sys::g_value_set_enum(value.to_glib_none_mut().0, this.to_glib())
    }
}

fn timestamp_mode_get_type() -> glib::Type {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    static mut TYPE: glib::Type = glib::Type::Invalid;

    ONCE.call_once(|| {
        use std::ffi;
        use std::ptr;

        static mut VALUES: [gobject_sys::GEnumValue; 4] = [
            gobject_sys::GEnumValue {
                value: TimestampMode::ReceiveTime as i32,
                value_name: b"Receive Time\0" as *const _ as *const _,
                value_nick: b"receive-time\0" as *const _ as *const _,
            },
            gobject_sys::GEnumValue {
                value: TimestampMode::Timecode as i32,
                value_name: b"NDI Timecode\0" as *const _ as *const _,
                value_nick: b"timecode\0" as *const _ as *const _,
            },
            gobject_sys::GEnumValue {
                value: TimestampMode::Timestamp as i32,
                value_name: b"NDI Timestamp\0" as *const _ as *const _,
                value_nick: b"timestamp\0" as *const _ as *const _,
            },
            gobject_sys::GEnumValue {
                value: 0,
                value_name: ptr::null(),
                value_nick: ptr::null(),
            },
        ];

        let name = ffi::CString::new("GstNdiTimestampMode").unwrap();
        unsafe {
            let type_ = gobject_sys::g_enum_register_static(name.as_ptr(), VALUES.as_ptr());
            TYPE = glib::translate::from_glib(type_);
        }
    });

    unsafe {
        assert_ne!(TYPE, glib::Type::Invalid);
        TYPE
    }
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
