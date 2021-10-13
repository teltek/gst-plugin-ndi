mod device_provider;
pub mod ndi;

#[cfg(feature = "sink-v1_14")]
#[path = "base/mod.rs"]
pub mod gst_base_compat;

#[cfg(any(feature = "sink", feature = "sink-v1_14"))]
mod ndisink;
#[cfg(any(feature = "sink", feature = "sink-v1_14"))]
mod ndisinkcombiner;
#[cfg(any(feature = "sink", feature = "sink-v1_14"))]
pub mod ndisinkmeta;
mod ndisrc;
mod ndisrcdemux;
pub mod ndisrcmeta;
pub mod ndisys;
pub mod receiver;

use crate::ndi::*;
use crate::ndisys::*;
use crate::receiver::*;

use std::time;

use once_cell::sync::Lazy;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy, glib::GEnum)]
#[repr(u32)]
#[genum(type_name = "GstNdiTimestampMode")]
pub enum TimestampMode {
    #[genum(name = "Receive Time / Timecode", nick = "receive-time-vs-timecode")]
    ReceiveTimeTimecode = 0,
    #[genum(name = "Receive Time / Timestamp", nick = "receive-time-vs-timestamp")]
    ReceiveTimeTimestamp = 1,
    #[genum(name = "NDI Timecode", nick = "timecode")]
    Timecode = 2,
    #[genum(name = "NDI Timestamp", nick = "timestamp")]
    Timestamp = 3,
    #[genum(name = "Receive Time", nick = "receive-time")]
    ReceiveTime = 4,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy, glib::GEnum)]
#[repr(u32)]
#[genum(type_name = "GstNdiRecvColorFormat")]
pub enum RecvColorFormat {
    #[genum(name = "BGRX or BGRA", nick = "bgrx-bgra")]
    BgrxBgra = 0,
    #[genum(name = "UYVY or BGRA", nick = "uyvy-bgra")]
    UyvyBgra = 1,
    #[genum(name = "RGBX or RGBA", nick = "rgbx-rgba")]
    RgbxRgba = 2,
    #[genum(name = "UYVY or RGBA", nick = "uyvy-rgba")]
    UyvyRgba = 3,
    #[genum(name = "Fastest", nick = "fastest")]
    Fastest = 4,
    #[genum(name = "Best", nick = "best")]
    Best = 5,
    #[cfg(feature = "advanced-sdk")]
    #[genum(name = "Compressed v1", nick = "compressed-v1")]
    CompressedV1 = 6,
    #[cfg(feature = "advanced-sdk")]
    #[genum(name = "Compressed v2", nick = "compressed-v2")]
    CompressedV2 = 7,
    #[cfg(feature = "advanced-sdk")]
    #[genum(name = "Compressed v3", nick = "compressed-v3")]
    CompressedV3 = 8,
    #[cfg(feature = "advanced-sdk")]
    #[genum(name = "Compressed v3 with audio", nick = "compressed-v3-with-audio")]
    CompressedV3WithAudio = 9,
    #[cfg(feature = "advanced-sdk")]
    #[genum(name = "Compressed v4", nick = "compressed-v4")]
    CompressedV4 = 10,
    #[cfg(feature = "advanced-sdk")]
    #[genum(name = "Compressed v4 with audio", nick = "compressed-v4-with-audio")]
    CompressedV4WithAudio = 11,
    #[cfg(feature = "advanced-sdk")]
    #[genum(name = "Compressed v5", nick = "compressed-v5")]
    CompressedV5 = 12,
    #[cfg(feature = "advanced-sdk")]
    #[genum(name = "Compressed v5 with audio", nick = "compressed-v5-with-audio")]
    CompressedV5WithAudio = 13,
}

impl From<RecvColorFormat> for NDIlib_recv_color_format_e {
    fn from(v: RecvColorFormat) -> Self {
        match v {
            RecvColorFormat::BgrxBgra => NDIlib_recv_color_format_BGRX_BGRA,
            RecvColorFormat::UyvyBgra => NDIlib_recv_color_format_UYVY_BGRA,
            RecvColorFormat::RgbxRgba => NDIlib_recv_color_format_RGBX_RGBA,
            RecvColorFormat::UyvyRgba => NDIlib_recv_color_format_UYVY_RGBA,
            RecvColorFormat::Fastest => NDIlib_recv_color_format_fastest,
            RecvColorFormat::Best => NDIlib_recv_color_format_best,
            #[cfg(feature = "advanced-sdk")]
            RecvColorFormat::CompressedV1 => NDIlib_recv_color_format_ex_compressed,
            #[cfg(feature = "advanced-sdk")]
            RecvColorFormat::CompressedV2 => NDIlib_recv_color_format_ex_compressed_v2,
            #[cfg(feature = "advanced-sdk")]
            RecvColorFormat::CompressedV3 => NDIlib_recv_color_format_ex_compressed_v3,
            #[cfg(feature = "advanced-sdk")]
            RecvColorFormat::CompressedV3WithAudio => {
                NDIlib_recv_color_format_ex_compressed_v3_with_audio
            }
            #[cfg(feature = "advanced-sdk")]
            RecvColorFormat::CompressedV4 => NDIlib_recv_color_format_ex_compressed_v4,
            #[cfg(feature = "advanced-sdk")]
            RecvColorFormat::CompressedV4WithAudio => {
                NDIlib_recv_color_format_ex_compressed_v4_with_audio
            }
            #[cfg(feature = "advanced-sdk")]
            RecvColorFormat::CompressedV5 => NDIlib_recv_color_format_ex_compressed_v5,
            #[cfg(feature = "advanced-sdk")]
            RecvColorFormat::CompressedV5WithAudio => {
                NDIlib_recv_color_format_ex_compressed_v5_with_audio
            }
        }
    }
}

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    if !ndi::initialize() {
        return Err(glib::bool_error!("Cannot initialize NDI"));
    }

    device_provider::register(plugin)?;

    ndisrc::register(plugin)?;
    ndisrcdemux::register(plugin)?;

    #[cfg(any(feature = "sink", feature = "sink-v1_14"))]
    {
        ndisinkcombiner::register(plugin)?;
        ndisink::register(plugin)?;
    }
    Ok(())
}

static DEFAULT_RECEIVER_NDI_NAME: Lazy<String> = Lazy::new(|| {
    format!(
        "GStreamer NDI Source {}-{}",
        env!("CARGO_PKG_VERSION"),
        env!("COMMIT_ID")
    )
});

#[cfg(feature = "reference-timestamps")]
static TIMECODE_CAPS: Lazy<gst::Caps> =
    Lazy::new(|| gst::Caps::new_simple("timestamp/x-ndi-timecode", &[]));
#[cfg(feature = "reference-timestamps")]
static TIMESTAMP_CAPS: Lazy<gst::Caps> =
    Lazy::new(|| gst::Caps::new_simple("timestamp/x-ndi-timestamp", &[]));

gst::plugin_define!(
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
