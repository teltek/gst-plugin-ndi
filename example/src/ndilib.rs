#![allow(non_camel_case_types, non_upper_case_globals)]

use std::ptr;

#[link(name = "ndi")]
extern "C" {
    pub fn NDIlib_initialize() -> bool;
    pub fn NDIlib_find_create_v2(
        p_create_settings: *const NDIlib_find_create_t,
    ) -> NDIlib_find_instance_t;
    pub fn NDIlib_find_get_current_sources(
        p_instance: NDIlib_find_instance_t,
        p_no_sources: *mut u32,
    ) -> *const NDIlib_source_t;
    pub fn NDIlib_recv_create_v3(
        p_create_settings: *const NDIlib_recv_create_v3_t,
    ) -> NDIlib_recv_instance_t;
    pub fn NDIlib_find_destroy(p_instance: NDIlib_find_instance_t);
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

//TODO review enum
pub type NDIlib_frame_type_e = i32;
pub const NDIlib_frame_type_none: NDIlib_frame_type_e = 0;
pub const NDIlib_frame_type_video: NDIlib_frame_type_e = 1;
pub const NDIlib_frame_type_audio: NDIlib_frame_type_e = 2;
pub const NDIlib_frame_type_metadata: NDIlib_frame_type_e = 3;
pub const NDIlib_frame_type_error: NDIlib_frame_type_e = 4;
pub const NDIlib_frame_type_status_change: NDIlib_frame_type_e = 100;

pub type NDIlib_recv_bandwidth_e = i32;
pub const NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_metadata_only: NDIlib_recv_bandwidth_e =
    -10;
pub const NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_audio_only: NDIlib_recv_bandwidth_e = 10;
pub const NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_lowest: NDIlib_recv_bandwidth_e = 0;
pub const NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_highest: NDIlib_recv_bandwidth_e = 100;

pub type NDIlib_recv_color_format_e = u32;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_BGRX_BGRA:
    NDIlib_recv_color_format_e = 0;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_UYVY_BGRA:
    NDIlib_recv_color_format_e = 1;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_RGBX_RGBA:
    NDIlib_recv_color_format_e = 2;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_UYVY_RGBA:
    NDIlib_recv_color_format_e = 3;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_fastest: NDIlib_recv_color_format_e =
    100;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_e_BGRX_BGRA:
    NDIlib_recv_color_format_e = 0;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_e_UYVY_BGRA:
    NDIlib_recv_color_format_e = 1;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_e_RGBX_RGBA:
    NDIlib_recv_color_format_e = 2;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_e_UYVY_RGBA:
    NDIlib_recv_color_format_e = 3;

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
            bandwidth: NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_highest,
            color_format: NDIlib_recv_color_format_e_NDIlib_recv_color_format_UYVY_BGRA,
            p_ndi_name: ptr::null(),
        }
    }
}

pub type NDIlib_recv_instance_t = *mut ::std::os::raw::c_void;

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
    pub FourCC: u32, //TODO enum
    pub frame_rate_N: ::std::os::raw::c_int,
    pub frame_rate_D: ::std::os::raw::c_int,
    pub picture_aspect_ratio: ::std::os::raw::c_float,
    pub frame_format_type: u32, //TODO enum
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
            FourCC: 0,
            frame_rate_N: 0,
            frame_rate_D: 0,
            picture_aspect_ratio: 0.0,
            frame_format_type: 0,
            timecode: 0, //NDIlib_send_timecode_synthesize,
            p_data: ptr::null(),
            line_stride_in_bytes: 0,
            p_metadata: ptr::null(),
            timestamp: 0,
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
            timecode: 0, //NDIlib_send_timecode_synthesize,
            p_data: ptr::null(),
            channel_stride_in_bytes: 0,
            p_metadata: ptr::null(),
            timestamp: 0,
        }
    }
}
