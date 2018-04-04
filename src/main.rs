use std::ptr;
use std::ffi::{CString, CStr};



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
    pub fn NDIlib_find_destroy(
        p_instance: NDIlib_find_instance_t,
    );
    pub fn NDIlib_recv_set_tally(
        p_instance: NDIlib_recv_instance_t,
        p_tally: *const NDIlib_tally_t,
    ) -> bool;
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
    pub p_ip_address: *const ::std::os::raw::c_char,
}


//TODO review enum
pub type NDIlib_recv_bandwidth_e = i32;
pub const NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_metadata_only : NDIlib_recv_bandwidth_e = -10;
pub const NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_audio_only : NDIlib_recv_bandwidth_e = 10;
pub const NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_lowest : NDIlib_recv_bandwidth_e = 0;
pub const NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_highest : NDIlib_recv_bandwidth_e = 100;


pub type NDIlib_recv_color_format_e = u32;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_BGRX_BGRA : NDIlib_recv_color_format_e = 0;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_UYVY_BGRA : NDIlib_recv_color_format_e = 1;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_RGBX_RGBA : NDIlib_recv_color_format_e = 2;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_UYVY_RGBA : NDIlib_recv_color_format_e = 3;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_fastest : NDIlib_recv_color_format_e = 100;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_e_BGRX_BGRA : NDIlib_recv_color_format_e = 0;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_e_UYVY_BGRA : NDIlib_recv_color_format_e = 1;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_e_RGBX_RGBA : NDIlib_recv_color_format_e = 2;
pub const NDIlib_recv_color_format_e_NDIlib_recv_color_format_e_UYVY_RGBA : NDIlib_recv_color_format_e = 3;

#[repr(C)]
#[derive(Debug,Copy,Clone)]
pub struct NDIlib_recv_create_v3_t {
    pub source_to_connect_to: NDIlib_source_t,
    pub color_format: NDIlib_recv_color_format_e,
    pub bandwidth: NDIlib_recv_bandwidth_e,
    pub allow_video_fields: bool,
    pub p_ndi_name: *const ::std::os::raw::c_char,
}

pub type NDIlib_recv_instance_t = *mut ::std::os::raw::c_void;


#[repr(C)]
#[derive(Debug,Copy,Clone)]
pub struct NDIlib_tally_t {
    pub on_program: bool,
    pub on_preview: bool,
}

#[repr(C)]
#[derive(Debug,Copy,Clone)]
pub struct NDIlib_metadata_frame_t {
    pub length: ::std::os::raw::c_int,
    pub timecode: i64,
    pub p_data: *mut ::std::os::raw::c_char,
}


fn main() {
    unsafe {
      if !NDIlib_initialize() {
          //TODO delete exits
          println!("Cannot run NDI: NDIlib_initialize error.");
          ::std::process::exit(1);
      }

      //TODO valores por defecto
      let mut NDI_find_create_desc = NDIlib_find_create_t {
          show_local_sources: false,
          p_groups: ptr::null(),
          p_extra_ips: ptr::null()
      };
      let pNDI_find = NDIlib_find_create_v2(&NDI_find_create_desc);
      if pNDI_find.is_null() {
          println!("Cannot run NDI: NDIlib_find_create_v2 error.");
          ::std::process::exit(1);
      }

      let mut no_sources: u32  = 0;
      let mut p_sources = ptr::null();
      while no_sources == 0 {
          p_sources = NDIlib_find_get_current_sources(pNDI_find, &mut no_sources as *mut u32);
      }


      // We need at least one source
      if p_sources.is_null() {
          println!("Error getting NDIlib_find_get_current_sources.");
          ::std::process::exit(1);
      }

      println!("no_source {}: Name '{}' Address '{}'",
        no_sources,
        CStr::from_ptr((*p_sources).p_ndi_name).to_string_lossy().into_owned(),
        CStr::from_ptr((*p_sources).p_ip_address).to_string_lossy().into_owned()
      );

      // We now have at least one source, so we create a receiver to look at it.
      // We tell it that we prefer YCbCr video since it is more efficient for us. If the source has an alpha channel
      // it will still be provided in BGRA
      let p_ndi_name = CString::new("Galicaster NDI Receiver").unwrap();
      let mut NDI_recv_create_desc = NDIlib_recv_create_v3_t {
          source_to_connect_to: *p_sources,
          allow_video_fields: false,
          bandwidth: NDIlib_recv_bandwidth_e_NDIlib_recv_bandwidth_lowest,
          color_format: NDIlib_recv_color_format_e_NDIlib_recv_color_format_BGRX_BGRA,
          p_ndi_name: p_ndi_name.as_ptr(), //ptr::null(),
      };


      let pNDI_recv = NDIlib_recv_create_v3(&NDI_recv_create_desc);
      if pNDI_recv.is_null() {
          println!("Cannot run NDI: NDIlib_recv_create_v3 error.");
          ::std::process::exit(1);
      }

      // Destroy the NDI finder. We needed to have access to the pointers to p_sources[0]
      NDIlib_find_destroy(pNDI_find);

      // We are now going to mark this source as being on program output for tally purposes (but not on preview)
      let tally_state = NDIlib_tally_t {
          on_program: true,
          on_preview: true,
      };
      NDIlib_recv_set_tally(pNDI_recv, &tally_state);


      // Enable Hardwqre Decompression support if this support has it. Please read the caveats in the documentation
      // regarding this. There are times in which it might reduce the performance although on small stream numbers
      // it almost always yields the same or better performance.
/*
      NDIlib_metadata_frame_t enable_hw_accel;
      enable_hw_accel.p_data = "<ndi_hwaccel enabled=\"true\"/>";
      NDIlib_recv_send_metadata(pNDI_recv, &enable_hw_accel);
*/





    }
    println!("Hello, world!");
}