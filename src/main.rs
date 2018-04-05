#![allow(non_upper_case_globals, non_snake_case)]


pub mod ndilib;

use std::ptr;
use std::ffi::{CString, CStr};

use ndilib::*;


fn main() {
    unsafe {
      if !NDIlib_initialize() {
          //TODO delete exits
          println!("Cannot run NDI: NDIlib_initialize error.");
          ::std::process::exit(1);
      }

      //TODO valores por defecto
      let NDI_find_create_desc = NDIlib_find_create_t {
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
      let NDI_recv_create_desc = NDIlib_recv_create_v3_t {
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
      let data = CString::new("<ndi_hwaccel enabled=\"true\"/>").unwrap();
      let enable_hw_accel = NDIlib_metadata_frame_t {
          length: data.to_bytes().len() as i32,
          timecode: 0,
          p_data: data.as_ptr(),
      };

      NDIlib_recv_send_metadata(pNDI_recv, &enable_hw_accel);

      loop {

          let video_frame = NDIlib_video_frame_v2_t {
              xres: 0,
              yres: 0,
              FourCC: 0,
              frame_rate_N: 0,
              frame_rate_D: 0,
              picture_aspect_ratio: 0.0,
              frame_format_type: 0,
              timecode: 0,
              p_data: ptr::null(),
              line_stride_in_bytes: 0,
              p_metadata: ptr::null(),
              timestamp: 0,
          };


          let audio_frame = NDIlib_audio_frame_v2_t {
              sample_rate: 0,
              no_channels: 0,
              no_samples: 0,
              timecode: 0,
              p_data: ptr::null(),
              channel_stride_in_bytes: 0,
              p_metadata: ptr::null(),
              timestamp: 0,
          };

          let metadata_frame = NDIlib_metadata_frame_t {
              length: 0,
              timecode: 0,
              p_data: ptr::null(),
          };

          let frame_type = NDIlib_recv_capture_v2(pNDI_recv, &video_frame, &audio_frame, &metadata_frame, 1000);


          match frame_type {
              NDIlib_frame_type_metadata => {
                  println!("Tengo metadata {} '{}'",
                      metadata_frame.length,
                      CStr::from_ptr(metadata_frame.p_data).to_string_lossy().into_owned(),
                  );
              },
              NDIlib_frame_type_error => {
                  println!("Tengo error {} '{}'",
                      metadata_frame.length,
                      CStr::from_ptr(metadata_frame.p_data).to_string_lossy().into_owned(),
                  );
                  break;
              },
              _ => println!("Tengo {}", frame_type),
          }

      }

    }
    println!("Exit");
}