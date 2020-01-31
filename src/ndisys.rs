#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]

#[cfg_attr(
    all(target_arch = "x86_64", target_os = "windows"),
    link(name = "Processing.NDI.Lib.x64")
)]
#[cfg_attr(
    all(target_arch = "x86", target_os = "windows"),
    link(name = "Processing.NDI.Lib.x86")
)]
#[cfg_attr(
    not(any(target_os = "windows", target_os = "macos")),
    link(name = "ndi")
)]
extern "C" {
    pub fn NDIlib_initialize() -> bool;
    pub fn NDIlib_destroy();
    pub fn NDIlib_find_create_v2(
        p_create_settings: *const NDIlib_find_create_t,
    ) -> NDIlib_find_instance_t;
    pub fn NDIlib_find_destroy(p_instance: NDIlib_find_instance_t);
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
    pub fn NDIlib_recv_destroy(p_instance: NDIlib_recv_instance_t);
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
        p_video_data: *mut NDIlib_video_frame_v2_t,
        p_audio_data: *mut NDIlib_audio_frame_v2_t,
        p_metadata: *mut NDIlib_metadata_frame_t,
        timeout_in_ms: u32,
    ) -> NDIlib_frame_type_e;
    pub fn NDIlib_recv_free_video_v2(
        p_instance: NDIlib_recv_instance_t,
        p_video_data: *mut NDIlib_video_frame_v2_t,
    );
    pub fn NDIlib_recv_free_audio_v2(
        p_instance: NDIlib_recv_instance_t,
        p_audio_data: *mut NDIlib_audio_frame_v2_t,
    );
    pub fn NDIlib_recv_free_metadata(
        p_instance: NDIlib_recv_instance_t,
        p_metadata: *mut NDIlib_metadata_frame_t,
    );
    pub fn NDIlib_recv_get_queue(
        p_instance: NDIlib_recv_instance_t,
        p_total: *mut NDIlib_recv_queue_t,
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

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_source_t {
    pub p_ndi_name: *const ::std::os::raw::c_char,
    pub p_url_address: *const ::std::os::raw::c_char,
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

pub type NDIlib_recv_bandwidth_e = i32;

pub const NDIlib_recv_bandwidth_metadata_only: NDIlib_recv_bandwidth_e = -10;
pub const NDIlib_recv_bandwidth_audio_only: NDIlib_recv_bandwidth_e = 10;
pub const NDIlib_recv_bandwidth_lowest: NDIlib_recv_bandwidth_e = 0;
pub const NDIlib_recv_bandwidth_highest: NDIlib_recv_bandwidth_e = 100;

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NDIlib_recv_color_format_e {
    NDIlib_recv_color_format_BGRX_BGRA = 0,
    NDIlib_recv_color_format_UYVY_BGRA = 1,
    NDIlib_recv_color_format_RGBX_RGBA = 2,
    NDIlib_recv_color_format_UYVY_RGBA = 3,
    NDIlib_recv_color_format_fastest = 100,
    NDIlib_recv_color_format_best = 101,
}

pub type NDIlib_FourCC_video_type_e = u32;
pub const NDIlib_FourCC_video_type_UYVY: NDIlib_FourCC_video_type_e = 0x59_56_59_55;
pub const NDIlib_FourCC_video_type_UYVA: NDIlib_FourCC_video_type_e = 0x41_56_56_55;
pub const NDIlib_FourCC_video_type_P216: NDIlib_FourCC_video_type_e = 0x36_31_32_50;
pub const NDIlib_FourCC_video_type_PA16: NDIlib_FourCC_video_type_e = 0x36_31_41_50;
pub const NDIlib_FourCC_video_type_YV12: NDIlib_FourCC_video_type_e = 0x32_31_56_59;
pub const NDIlib_FourCC_video_type_I420: NDIlib_FourCC_video_type_e = 0x30_32_34_49;
pub const NDIlib_FourCC_video_type_NV12: NDIlib_FourCC_video_type_e = 0x32_31_56_4e;
pub const NDIlib_FourCC_video_type_BGRA: NDIlib_FourCC_video_type_e = 0x41_52_47_42;
pub const NDIlib_FourCC_video_type_BGRX: NDIlib_FourCC_video_type_e = 0x58_52_47_42;
pub const NDIlib_FourCC_video_type_RGBA: NDIlib_FourCC_video_type_e = 0x41_42_47_52;
pub const NDIlib_FourCC_video_type_RGBX: NDIlib_FourCC_video_type_e = 0x58_42_47_52;

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NDIlib_frame_format_type_e {
    NDIlib_frame_format_type_progressive = 1,
    NDIlib_frame_format_type_interleaved = 0,
    NDIlib_frame_format_type_field_0 = 2,
    NDIlib_frame_format_type_field_1 = 3,
}

pub const NDIlib_send_timecode_synthesize: i64 = ::std::i64::MAX;
pub const NDIlib_recv_timestamp_undefined: i64 = ::std::i64::MAX;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_recv_create_v3_t {
    pub source_to_connect_to: NDIlib_source_t,
    pub color_format: NDIlib_recv_color_format_e,
    pub bandwidth: NDIlib_recv_bandwidth_e,
    pub allow_video_fields: bool,
    pub p_ndi_recv_name: *const ::std::os::raw::c_char,
}

pub type NDIlib_recv_instance_t = *mut ::std::os::raw::c_void;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_tally_t {
    pub on_program: bool,
    pub on_preview: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_recv_queue_t {
    pub video_frames: i32,
    pub audio_frames: i32,
    pub metadata_frames: i32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_metadata_frame_t {
    pub length: ::std::os::raw::c_int,
    pub timecode: i64,
    pub p_data: *const ::std::os::raw::c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct NDIlib_video_frame_v2_t {
    pub xres: ::std::os::raw::c_int,
    pub yres: ::std::os::raw::c_int,
    pub FourCC: NDIlib_FourCC_video_type_e,
    pub frame_rate_N: ::std::os::raw::c_int,
    pub frame_rate_D: ::std::os::raw::c_int,
    pub picture_aspect_ratio: ::std::os::raw::c_float,
    pub frame_format_type: NDIlib_frame_format_type_e,
    pub timecode: i64,
    pub p_data: *const ::std::os::raw::c_char,
    pub line_stride_or_data_size_in_bytes: ::std::os::raw::c_int,
    pub p_metadata: *const ::std::os::raw::c_char,
    pub timestamp: i64,
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
    pub p_data: *mut i16,
}
