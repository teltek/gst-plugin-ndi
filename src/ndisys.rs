#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]

use std::ptr;

#[cfg_attr(all(target_arch = "x86_64", target_os = "windows"), link(name = "Processing.NDI.Lib.x64"))]
#[cfg_attr(all(target_arch = "x86", target_os = "windows"), link(name = "Processing.NDI.Lib.x86"))]
#[cfg_attr(not(any(target_os = "windows", target_os = "macos")), link(name = "ndi"))]
extern "C" {
    pub fn NDIlib_initialize() -> bool;
    pub fn NDIlib_find_create_v2(
        p_create_settings: *const NDIlib_find_create_t,
    ) -> NDIlib_find_instance_t;
    pub fn NDIlib_find_wait_for_sources(
        p_instance: NDIlib_find_instance_t,
        timeout_in_ms: u32,
    ) -> bool;
    pub fn NDIlib_find_get_current_sources(
        p_instance: NDIlib_find_instance_t,
        p_no_sources: *mut u32,
    ) -> *const NDIlib_source_t;
    pub fn NDIlib_recv_create_v3(
        p_create_settings: *const NDIlib_recv_create_v3_t,
    ) -> NDIlib_recv_instance_t;
    pub fn NDIlib_find_destroy(p_instance: NDIlib_find_instance_t);
    pub fn NDIlib_recv_destroy(p_instance: NDIlib_recv_instance_t);
    pub fn NDIlib_destroy();
    pub fn NDIlib_recv_set_tally(
        p_instance: NDIlib_recv_instance_t,
        p_tally: *const NDIlib_tally_t,
    ) -> bool;
    pub fn NDIlib_recv_send_metadata(
        p_instance: NDIlib_recv_instance_t,
        p_metadata: *const NDIlib_metadata_frame_t,
    ) -> bool;
    pub fn NDIlib_recv_capture_v2(
        p_instance: NDIlib_recv_instance_t,
        p_video_data: *const NDIlib_video_frame_v2_t,
        p_audio_data: *const NDIlib_audio_frame_v2_t,
        p_metadata: *const NDIlib_metadata_frame_t,
        timeout_in_ms: u32,
    ) -> NDIlib_frame_type_e;
    pub fn NDIlib_recv_free_video_v2(
        p_instance: NDIlib_recv_instance_t,
        p_video_data: *const NDIlib_video_frame_v2_t,
    );
    pub fn NDIlib_recv_free_audio_v2(
        p_instance: NDIlib_recv_instance_t,
        p_audio_data: *const NDIlib_audio_frame_v2_t,
    );
    pub fn NDIlib_recv_free_metadata(
        p_instance: NDIlib_recv_instance_t,
        p_metadata: *const NDIlib_metadata_frame_t,
    );
}

pub type NDIlib_find_instance_t = *mut ::std::os::raw::c_void;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_find_create_t {
    pub show_local_sources: bool,
    pub p_groups: *const ::std::os::raw::c_char,
    pub p_extra_ips: *const ::std::os::raw::c_char,
}

impl Default for NDIlib_find_create_t {
    fn default() -> Self {
        NDIlib_find_create_t {
            show_local_sources: true,
            p_groups: ptr::null(),
            p_extra_ips: ptr::null(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_source_t {
    pub p_ndi_name: *const ::std::os::raw::c_char,
    pub p_ip_address: *const ::std::os::raw::c_char,
}

impl Default for NDIlib_source_t {
    fn default() -> Self {
        NDIlib_source_t {
            p_ndi_name: ptr::null(),
            p_ip_address: ptr::null(),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NDIlib_frame_type_e {
    NDIlib_frame_type_none = 0,
    NDIlib_frame_type_video = 1,
    NDIlib_frame_type_audio = 2,
    NDIlib_frame_type_metadata = 3,
    NDIlib_frame_type_error = 4,
    NDIlib_frame_type_status_change = 100,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NDIlib_recv_bandwidth_e {
    NDIlib_recv_bandwidth_metadata_only = -10,
    NDIlib_recv_bandwidth_audio_only = 10,
    NDIlib_recv_bandwidth_lowest = 0,
    NDIlib_recv_bandwidth_highest = 100,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NDIlib_recv_color_format_e {
    NDIlib_recv_color_format_BGRX_BGRA = 0,
    NDIlib_recv_color_format_UYVY_BGRA = 1,
    NDIlib_recv_color_format_RGBX_RGBA = 2,
    NDIlib_recv_color_format_UYVY_RGBA = 3,
    NDIlib_recv_color_format_fastest = 100,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NDIlib_FourCC_type_e {
    NDIlib_FourCC_type_UYVY = 1_498_831_189,
    NDIlib_FourCC_type_BGRA = 1_095_911_234,
    NDIlib_FourCC_type_BGRX = 1_481_787_202,
    NDIlib_FourCC_type_RGBA = 1_094_862_674,
    NDIlib_FourCC_type_RGBX = 1_480_738_642,
    NDIlib_FourCC_type_UYVA = 1_096_178_005,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NDIlib_frame_format_type_e {
    NDIlib_frame_format_type_progressive = 1,
    NDIlib_frame_format_type_interleaved = 0,
    NDIlib_frame_format_type_field_0 = 2,
    NDIlib_frame_format_type_field_1 = 3,
}

pub const NDIlib_send_timecode_synthesize: i64 = ::std::i64::MAX;
pub const NDIlib_send_timecode_empty: i64 = 0;
pub const NDIlib_recv_timestamp_undefined: i64 = ::std::i64::MAX;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_recv_create_v3_t {
    pub source_to_connect_to: NDIlib_source_t,
    pub color_format: NDIlib_recv_color_format_e,
    pub bandwidth: NDIlib_recv_bandwidth_e,
    pub allow_video_fields: bool,
    pub p_ndi_name: *const ::std::os::raw::c_char,
}

impl Default for NDIlib_recv_create_v3_t {
    fn default() -> Self {
        NDIlib_recv_create_v3_t {
            source_to_connect_to: Default::default(),
            allow_video_fields: true,
            bandwidth: NDIlib_recv_bandwidth_e::NDIlib_recv_bandwidth_highest,
            color_format: NDIlib_recv_color_format_e::NDIlib_recv_color_format_UYVY_BGRA,
            p_ndi_name: ptr::null(),
        }
    }
}

pub type NDIlib_recv_instance_t = *mut ::std::os::raw::c_void;

//Rust wrapper around *mut ::std::os::raw::c_void
pub struct NdiInstance {
    pub recv: NDIlib_recv_instance_t,
    // pub audio: bool,
}

unsafe impl ::std::marker::Send for NdiInstance {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_tally_t {
    pub on_program: bool,
    pub on_preview: bool,
}

impl Default for NDIlib_tally_t {
    fn default() -> Self {
        NDIlib_tally_t {
            on_program: false,
            on_preview: false,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_metadata_frame_t {
    pub length: ::std::os::raw::c_int,
    pub timecode: i64,
    pub p_data: *const ::std::os::raw::c_char,
}

impl Default for NDIlib_metadata_frame_t {
    fn default() -> Self {
        NDIlib_metadata_frame_t {
            length: 0,
            timecode: 0, //NDIlib_send_timecode_synthesize,
            p_data: ptr::null(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_video_frame_v2_t {
    pub xres: ::std::os::raw::c_int,
    pub yres: ::std::os::raw::c_int,
    pub FourCC: NDIlib_FourCC_type_e,
    pub frame_rate_N: ::std::os::raw::c_int,
    pub frame_rate_D: ::std::os::raw::c_int,
    pub picture_aspect_ratio: ::std::os::raw::c_float,
    pub frame_format_type: NDIlib_frame_format_type_e,
    pub timecode: i64,
    pub p_data: *const ::std::os::raw::c_char,
    pub line_stride_in_bytes: ::std::os::raw::c_int,
    pub p_metadata: *const ::std::os::raw::c_char,
    pub timestamp: i64,
}

impl Default for NDIlib_video_frame_v2_t {
    fn default() -> Self {
        NDIlib_video_frame_v2_t {
            xres: 0,
            yres: 0,
            FourCC: NDIlib_FourCC_type_e::NDIlib_FourCC_type_UYVY,
            frame_rate_N: 30000,
            frame_rate_D: 1001,
            picture_aspect_ratio: 0.0,
            frame_format_type: NDIlib_frame_format_type_e::NDIlib_frame_format_type_progressive,
            timecode: NDIlib_send_timecode_synthesize,
            p_data: ptr::null(),
            line_stride_in_bytes: 0,
            p_metadata: ptr::null(),
            timestamp: NDIlib_send_timecode_empty,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_audio_frame_v2_t {
    pub sample_rate: ::std::os::raw::c_int,
    pub no_channels: ::std::os::raw::c_int,
    pub no_samples: ::std::os::raw::c_int,
    pub timecode: i64,
    pub p_data: *const ::std::os::raw::c_float,
    pub channel_stride_in_bytes: ::std::os::raw::c_int,
    pub p_metadata: *const ::std::os::raw::c_char,
    pub timestamp: i64,
}

impl Default for NDIlib_audio_frame_v2_t {
    fn default() -> Self {
        NDIlib_audio_frame_v2_t {
            sample_rate: 48000,
            no_channels: 2,
            no_samples: 0,
            timecode: NDIlib_send_timecode_synthesize,
            p_data: ptr::null(),
            channel_stride_in_bytes: 0,
            p_metadata: ptr::null(),
            timestamp: NDIlib_send_timecode_empty,
        }
    }
}

extern "C" {
    pub fn NDIlib_util_audio_to_interleaved_16s_v2(
        p_src: *const NDIlib_audio_frame_v2_t,
        p_dst: *mut NDIlib_audio_frame_interleaved_16s_t,
    );

    pub fn NDIlib_util_audio_from_interleaved_16s_v2(
        p_src: *const NDIlib_audio_frame_interleaved_16s_t,
        p_dst: *mut NDIlib_audio_frame_v2_t,
    );
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_audio_frame_interleaved_16s_t {
    pub sample_rate: ::std::os::raw::c_int,
    pub no_channels: ::std::os::raw::c_int,
    pub no_samples: ::std::os::raw::c_int,
    pub timecode: i64,
    pub reference_level: ::std::os::raw::c_int,
    pub p_data: *mut ::std::os::raw::c_short,
}

impl Default for NDIlib_audio_frame_interleaved_16s_t {
    fn default() -> Self {
        NDIlib_audio_frame_interleaved_16s_t {
            sample_rate: 48000,
            no_channels: 2,
            no_samples: 0,
            timecode: NDIlib_send_timecode_synthesize,
            reference_level: 0,
            p_data: ptr::null_mut(),
        }
    }
}
