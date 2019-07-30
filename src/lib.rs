#[macro_use]
extern crate glib;
use glib::prelude::*;
#[macro_use]
extern crate gstreamer as gst;
extern crate gstreamer_audio as gst_audio;
extern crate gstreamer_base as gst_base;
extern crate gstreamer_sys as gst_sys;
extern crate gstreamer_video as gst_video;

#[macro_use]
extern crate lazy_static;
extern crate byte_slice_cast;

pub mod ndi;
mod ndiaudiosrc;
pub mod ndisys;
mod ndivideosrc;
pub mod receiver;

use crate::ndi::*;
use crate::ndisys::*;
use crate::receiver::*;

use std::collections::HashMap;
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

lazy_static! {
    static ref DEFAULT_RECEIVER_NDI_NAME: String = {
        format!(
            "GStreamer NDI Source {}-{}",
            env!("CARGO_PKG_VERSION"),
            env!("COMMIT_ID")
        )
    };
}

#[cfg(feature = "reference-timestamps")]
lazy_static! {
    static ref TIMECODE_CAPS: gst::Caps =
        { gst::Caps::new_simple("timestamp/x-ndi-timecode", &[]) };
    static ref TIMESTAMP_CAPS: gst::Caps =
        { gst::Caps::new_simple("timestamp/x-ndi-timestamp", &[]) };
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
