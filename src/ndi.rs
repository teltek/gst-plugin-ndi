use crate::ndisys;
use crate::ndisys::*;
use std::ffi;
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};

pub fn initialize() -> bool {
    unsafe { NDIlib_initialize() }
}

#[derive(Debug)]
pub struct FindBuilder<'a> {
    show_local_sources: bool,
    groups: Option<&'a str>,
    extra_ips: Option<&'a str>,
}

impl<'a> Default for FindBuilder<'a> {
    fn default() -> Self {
        Self {
            show_local_sources: true,
            groups: None,
            extra_ips: None,
        }
    }
}

impl<'a> FindBuilder<'a> {
    pub fn show_local_sources(self, show_local_sources: bool) -> Self {
        Self {
            show_local_sources,
            ..self
        }
    }

    pub fn groups(self, groups: &'a str) -> Self {
        Self {
            groups: Some(groups),
            ..self
        }
    }

    pub fn extra_ips(self, extra_ips: &'a str) -> Self {
        Self {
            extra_ips: Some(extra_ips),
            ..self
        }
    }

    pub fn build(self) -> Option<FindInstance> {
        let groups = self.groups.map(|s| ffi::CString::new(s).unwrap());
        let extra_ips = self.extra_ips.map(|s| ffi::CString::new(s).unwrap());

        unsafe {
            let ptr = NDIlib_find_create_v2(&NDIlib_find_create_t {
                show_local_sources: self.show_local_sources,
                p_groups: groups.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null()),
                p_extra_ips: extra_ips
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(ptr::null()),
            });
            if ptr.is_null() {
                None
            } else {
                Some(FindInstance(ptr::NonNull::new_unchecked(ptr)))
            }
        }
    }
}

#[derive(Debug)]
pub struct FindInstance(ptr::NonNull<::std::os::raw::c_void>);
unsafe impl Send for FindInstance {}

impl FindInstance {
    pub fn builder<'a>() -> FindBuilder<'a> {
        FindBuilder::default()
    }

    pub fn wait_for_sources(&mut self, timeout_in_ms: u32) -> bool {
        unsafe { NDIlib_find_wait_for_sources(self.0.as_ptr(), timeout_in_ms) }
    }

    pub fn get_current_sources(&mut self) -> Vec<Source> {
        unsafe {
            let mut no_sources = mem::MaybeUninit::uninit();
            let sources_ptr =
                NDIlib_find_get_current_sources(self.0.as_ptr(), no_sources.as_mut_ptr());
            let no_sources = no_sources.assume_init();

            if sources_ptr.is_null() || no_sources == 0 {
                return vec![];
            }

            let mut sources = vec![];
            for i in 0..no_sources {
                sources.push(Source::Borrowed(
                    ptr::NonNull::new_unchecked(sources_ptr.add(i as usize) as *mut _),
                    self,
                ));
            }

            sources
        }
    }
}

impl Drop for FindInstance {
    fn drop(&mut self) {
        unsafe {
            NDIlib_find_destroy(self.0.as_mut());
        }
    }
}

#[derive(Debug)]
pub enum Source<'a> {
    Borrowed(ptr::NonNull<NDIlib_source_t>, &'a FindInstance),
    Owned(NDIlib_source_t, ffi::CString, ffi::CString),
}

unsafe impl<'a> Send for Source<'a> {}

impl<'a> Source<'a> {
    pub fn ndi_name(&self) -> &str {
        unsafe {
            let ptr = match *self {
                Source::Borrowed(ptr, _) => &*ptr.as_ptr(),
                Source::Owned(ref source, _, _) => source,
            };

            assert!(!ptr.p_ndi_name.is_null());
            ffi::CStr::from_ptr(ptr.p_ndi_name).to_str().unwrap()
        }
    }

    pub fn url_address(&self) -> &str {
        unsafe {
            let ptr = match *self {
                Source::Borrowed(ptr, _) => &*ptr.as_ptr(),
                Source::Owned(ref source, _, _) => source,
            };

            assert!(!ptr.p_url_address.is_null());
            ffi::CStr::from_ptr(ptr.p_url_address).to_str().unwrap()
        }
    }

    fn ndi_name_ptr(&self) -> *const ::std::os::raw::c_char {
        unsafe {
            match *self {
                Source::Borrowed(ptr, _) => ptr.as_ref().p_ndi_name,
                Source::Owned(_, ref ndi_name, _) => ndi_name.as_ptr(),
            }
        }
    }

    fn url_address_ptr(&self) -> *const ::std::os::raw::c_char {
        unsafe {
            match *self {
                Source::Borrowed(ptr, _) => ptr.as_ref().p_url_address,
                Source::Owned(_, _, ref url_address) => url_address.as_ptr(),
            }
        }
    }

    pub fn to_owned<'b>(&self) -> Source<'b> {
        unsafe {
            let (ndi_name, url_address) = match *self {
                Source::Borrowed(ptr, _) => (ptr.as_ref().p_ndi_name, ptr.as_ref().p_url_address),
                Source::Owned(_, ref ndi_name, ref url_address) => {
                    (ndi_name.as_ptr(), url_address.as_ptr())
                }
            };

            let ndi_name = ffi::CString::new(ffi::CStr::from_ptr(ndi_name).to_bytes()).unwrap();
            let url_address =
                ffi::CString::new(ffi::CStr::from_ptr(url_address).to_bytes()).unwrap();

            Source::Owned(
                NDIlib_source_t {
                    p_ndi_name: ndi_name.as_ptr(),
                    p_url_address: url_address.as_ptr(),
                },
                ndi_name,
                url_address,
            )
        }
    }
}

#[derive(Debug)]
pub struct RecvBuilder<'a> {
    source_to_connect_to: &'a Source<'a>,
    allow_video_fields: bool,
    bandwidth: NDIlib_recv_bandwidth_e,
    color_format: NDIlib_recv_color_format_e,
    ndi_name: &'a str,
}

impl<'a> RecvBuilder<'a> {
    pub fn allow_video_fields(self, allow_video_fields: bool) -> Self {
        Self {
            allow_video_fields,
            ..self
        }
    }

    pub fn bandwidth(self, bandwidth: NDIlib_recv_bandwidth_e) -> Self {
        Self { bandwidth, ..self }
    }

    pub fn color_format(self, color_format: NDIlib_recv_color_format_e) -> Self {
        Self {
            color_format,
            ..self
        }
    }

    pub fn build(self) -> Option<RecvInstance> {
        unsafe {
            let ndi_name = ffi::CString::new(self.ndi_name).unwrap();
            let ptr = NDIlib_recv_create_v3(&NDIlib_recv_create_v3_t {
                source_to_connect_to: NDIlib_source_t {
                    p_ndi_name: self.source_to_connect_to.ndi_name_ptr(),
                    p_url_address: self.source_to_connect_to.url_address_ptr(),
                },
                allow_video_fields: self.allow_video_fields,
                bandwidth: self.bandwidth,
                color_format: self.color_format,
                p_ndi_recv_name: ndi_name.as_ptr(),
            });

            if ptr.is_null() {
                None
            } else {
                Some(RecvInstance(Arc::new((
                    RecvInstanceInner(ptr::NonNull::new_unchecked(ptr)),
                    Mutex::new(()),
                ))))
            }
        }
    }
}

// Any access to the RecvInstanceInner apart from calling the capture function must be protected by
// the mutex
#[derive(Debug, Clone)]
pub struct RecvInstance(Arc<(RecvInstanceInner, Mutex<()>)>);

#[derive(Debug)]
struct RecvInstanceInner(ptr::NonNull<::std::os::raw::c_void>);
unsafe impl Send for RecvInstanceInner {}

// Not 100% true but we ensure safety with the mutex. The documentation says that only the
// capturing itself can be performed from multiple threads at once safely.
unsafe impl Sync for RecvInstanceInner {}

impl RecvInstance {
    pub fn builder<'a>(source_to_connect_to: &'a Source, ndi_name: &'a str) -> RecvBuilder<'a> {
        RecvBuilder {
            source_to_connect_to,
            allow_video_fields: true,
            bandwidth: NDIlib_recv_bandwidth_highest,
            color_format: NDIlib_recv_color_format_e::NDIlib_recv_color_format_UYVY_BGRA,
            ndi_name,
        }
    }

    pub fn set_tally(&self, tally: &Tally) -> bool {
        unsafe {
            let _lock = (self.0).1.lock().unwrap();
            NDIlib_recv_set_tally(((self.0).0).0.as_ptr(), &tally.0)
        }
    }

    pub fn send_metadata(&self, metadata: &MetadataFrame) -> bool {
        unsafe {
            let _lock = (self.0).1.lock().unwrap();
            NDIlib_recv_send_metadata(((self.0).0).0.as_ptr(), metadata.as_ptr())
        }
    }

    pub fn get_queue(&self) -> Queue {
        unsafe {
            let _lock = (self.0).1.lock().unwrap();
            let mut queue = mem::MaybeUninit::uninit();
            NDIlib_recv_get_queue(((self.0).0).0.as_ptr(), queue.as_mut_ptr());
            Queue(queue.assume_init())
        }
    }

    pub fn capture(
        &self,
        video: bool,
        audio: bool,
        metadata: bool,
        timeout_in_ms: u32,
    ) -> Result<Option<Frame>, ()> {
        unsafe {
            // Capturing from multiple threads at once is safe according to the documentation
            let ptr = ((self.0).0).0.as_ptr();

            let mut video_frame = mem::zeroed();
            let mut audio_frame = mem::zeroed();
            let mut metadata_frame = mem::zeroed();

            let res = NDIlib_recv_capture_v2(
                ptr,
                if video {
                    &mut video_frame
                } else {
                    ptr::null_mut()
                },
                if audio {
                    &mut audio_frame
                } else {
                    ptr::null_mut()
                },
                if metadata {
                    &mut metadata_frame
                } else {
                    ptr::null_mut()
                },
                timeout_in_ms,
            );

            match res {
                NDIlib_frame_type_e::NDIlib_frame_type_audio => {
                    assert!(audio);
                    Ok(Some(Frame::Audio(AudioFrame::Borrowed(audio_frame, self))))
                }
                NDIlib_frame_type_e::NDIlib_frame_type_video => {
                    assert!(video);
                    Ok(Some(Frame::Video(VideoFrame::Borrowed(video_frame, self))))
                }
                NDIlib_frame_type_e::NDIlib_frame_type_metadata => {
                    assert!(metadata);
                    Ok(Some(Frame::Metadata(MetadataFrame::Borrowed(
                        metadata_frame,
                        self,
                    ))))
                }
                NDIlib_frame_type_e::NDIlib_frame_type_error => Err(()),
                _ => Ok(None),
            }
        }
    }
}

impl Drop for RecvInstanceInner {
    fn drop(&mut self) {
        unsafe { NDIlib_recv_destroy(self.0.as_ptr() as *mut _) }
    }
}

#[derive(Debug)]
pub struct Tally(NDIlib_tally_t);
unsafe impl Send for Tally {}

impl Default for Tally {
    fn default() -> Self {
        Self(NDIlib_tally_t {
            on_program: true,
            on_preview: false,
        })
    }
}

impl Tally {
    pub fn new(on_program: bool, on_preview: bool) -> Self {
        Self(NDIlib_tally_t {
            on_program,
            on_preview,
        })
    }

    pub fn on_program(&self) -> bool {
        self.0.on_program
    }

    pub fn on_preview(&self) -> bool {
        self.0.on_preview
    }
}

#[derive(Debug)]
pub enum Frame<'a> {
    Video(VideoFrame<'a>),
    Audio(AudioFrame<'a>),
    Metadata(MetadataFrame<'a>),
}

#[derive(Debug)]
pub enum VideoFrame<'a> {
    //Owned(NDIlib_video_frame_v2_t, Option<ffi::CString>, Option<Vec<u8>>),
    Borrowed(NDIlib_video_frame_v2_t, &'a RecvInstance),
}

impl<'a> VideoFrame<'a> {
    pub fn xres(&self) -> i32 {
        match self {
            VideoFrame::Borrowed(ref frame, _) => frame.xres,
        }
    }

    pub fn yres(&self) -> i32 {
        match self {
            VideoFrame::Borrowed(ref frame, _) => frame.yres,
        }
    }

    pub fn fourcc(&self) -> NDIlib_FourCC_video_type_e {
        match self {
            VideoFrame::Borrowed(ref frame, _) => frame.FourCC,
        }
    }

    pub fn frame_rate(&self) -> (i32, i32) {
        match self {
            VideoFrame::Borrowed(ref frame, _) => (frame.frame_rate_N, frame.frame_rate_D),
        }
    }

    pub fn picture_aspect_ratio(&self) -> f32 {
        match self {
            VideoFrame::Borrowed(ref frame, _) => frame.picture_aspect_ratio,
        }
    }

    pub fn frame_format_type(&self) -> NDIlib_frame_format_type_e {
        match self {
            VideoFrame::Borrowed(ref frame, _) => frame.frame_format_type,
        }
    }

    pub fn timecode(&self) -> i64 {
        match self {
            VideoFrame::Borrowed(ref frame, _) => frame.timecode,
        }
    }

    pub fn data(&self) -> &[u8] {
        // FIXME: Unclear if this is correct. Needs to be validated against an actual
        // interlaced stream
        let frame_size = if self.frame_format_type()
            == NDIlib_frame_format_type_e::NDIlib_frame_format_type_field_0
            || self.frame_format_type()
                == NDIlib_frame_format_type_e::NDIlib_frame_format_type_field_1
        {
            self.yres() * self.line_stride_or_data_size_in_bytes() / 2
        } else {
            self.yres() * self.line_stride_or_data_size_in_bytes()
        };

        unsafe {
            use std::slice;
            match self {
                VideoFrame::Borrowed(ref frame, _) => {
                    slice::from_raw_parts(frame.p_data as *const u8, frame_size as usize)
                }
            }
        }
    }

    pub fn line_stride_or_data_size_in_bytes(&self) -> i32 {
        match self {
            VideoFrame::Borrowed(ref frame, _) => {
                let stride = frame.line_stride_or_data_size_in_bytes;

                if stride != 0 {
                    return stride;
                }

                let xres = frame.xres;

                match frame.FourCC {
                    ndisys::NDIlib_FourCC_video_type_UYVY
                    | ndisys::NDIlib_FourCC_video_type_UYVA
                    | ndisys::NDIlib_FourCC_video_type_YV12
                    | ndisys::NDIlib_FourCC_video_type_NV12
                    | ndisys::NDIlib_FourCC_video_type_I420
                    | ndisys::NDIlib_FourCC_video_type_BGRA
                    | ndisys::NDIlib_FourCC_video_type_BGRX
                    | ndisys::NDIlib_FourCC_video_type_RGBA
                    | ndisys::NDIlib_FourCC_video_type_RGBX => xres,
                    ndisys::NDIlib_FourCC_video_type_P216
                    | ndisys::NDIlib_FourCC_video_type_PA16 => 2 * xres,
                    _ => 0,
                }
            }
        }
    }

    pub fn metadata(&self) -> Option<&str> {
        unsafe {
            match self {
                VideoFrame::Borrowed(ref frame, _) => {
                    if frame.p_metadata.is_null() {
                        None
                    } else {
                        Some(ffi::CStr::from_ptr(frame.p_metadata).to_str().unwrap())
                    }
                }
            }
        }
    }

    pub fn timestamp(&self) -> i64 {
        match self {
            VideoFrame::Borrowed(ref frame, _) => frame.timestamp,
        }
    }

    pub fn as_ptr(&self) -> *const NDIlib_video_frame_v2_t {
        match self {
            VideoFrame::Borrowed(ref frame, _) => frame,
        }
    }
}

impl<'a> Drop for VideoFrame<'a> {
    #[allow(irrefutable_let_patterns)]
    fn drop(&mut self) {
        if let VideoFrame::Borrowed(ref mut frame, ref recv) = *self {
            unsafe {
                NDIlib_recv_free_video_v2(((recv.0).0).0.as_ptr() as *mut _, frame);
            }
        }
    }
}

#[derive(Debug)]
pub enum AudioFrame<'a> {
    //Owned(NDIlib_audio_frame_v2_t, Option<ffi::CString>, Option<Vec<u8>>),
    Borrowed(NDIlib_audio_frame_v2_t, &'a RecvInstance),
}

impl<'a> AudioFrame<'a> {
    pub fn sample_rate(&self) -> i32 {
        match self {
            AudioFrame::Borrowed(ref frame, _) => frame.sample_rate,
        }
    }

    pub fn no_channels(&self) -> i32 {
        match self {
            AudioFrame::Borrowed(ref frame, _) => frame.no_channels,
        }
    }

    pub fn no_samples(&self) -> i32 {
        match self {
            AudioFrame::Borrowed(ref frame, _) => frame.no_samples,
        }
    }

    pub fn timecode(&self) -> i64 {
        match self {
            AudioFrame::Borrowed(ref frame, _) => frame.timecode,
        }
    }

    pub fn data(&self) -> &[u8] {
        unsafe {
            use std::slice;
            match self {
                AudioFrame::Borrowed(ref frame, _) => slice::from_raw_parts(
                    frame.p_data as *const u8,
                    (frame.no_samples * frame.channel_stride_in_bytes) as usize,
                ),
            }
        }
    }

    pub fn channel_stride_in_bytes(&self) -> i32 {
        match self {
            AudioFrame::Borrowed(ref frame, _) => frame.channel_stride_in_bytes,
        }
    }

    pub fn metadata(&self) -> Option<&str> {
        unsafe {
            match self {
                AudioFrame::Borrowed(ref frame, _) => {
                    if frame.p_metadata.is_null() {
                        None
                    } else {
                        Some(ffi::CStr::from_ptr(frame.p_metadata).to_str().unwrap())
                    }
                }
            }
        }
    }

    pub fn timestamp(&self) -> i64 {
        match self {
            AudioFrame::Borrowed(ref frame, _) => frame.timestamp,
        }
    }

    pub fn as_ptr(&self) -> *const NDIlib_audio_frame_v2_t {
        match self {
            AudioFrame::Borrowed(ref frame, _) => frame,
        }
    }

    pub fn copy_to_interleaved_16s(&self, data: &mut [i16]) {
        assert_eq!(
            data.len(),
            (self.no_samples() * self.no_channels()) as usize
        );

        let mut dst = NDIlib_audio_frame_interleaved_16s_t {
            sample_rate: self.sample_rate(),
            no_channels: self.no_channels(),
            no_samples: self.no_samples(),
            timecode: self.timecode(),
            reference_level: 0,
            p_data: data.as_mut_ptr(),
        };

        unsafe {
            NDIlib_util_audio_to_interleaved_16s_v2(self.as_ptr(), &mut dst);
        }
    }
}

impl<'a> Drop for AudioFrame<'a> {
    #[allow(irrefutable_let_patterns)]
    fn drop(&mut self) {
        if let AudioFrame::Borrowed(ref mut frame, ref recv) = *self {
            unsafe {
                NDIlib_recv_free_audio_v2(((recv.0).0).0.as_ptr() as *mut _, frame);
            }
        }
    }
}

#[derive(Debug)]
pub enum MetadataFrame<'a> {
    Owned(NDIlib_metadata_frame_t, Option<ffi::CString>),
    Borrowed(NDIlib_metadata_frame_t, &'a RecvInstance),
}

impl<'a> MetadataFrame<'a> {
    pub fn new(timecode: i64, data: Option<&str>) -> Self {
        let data = data.map(|s| ffi::CString::new(s).unwrap());

        MetadataFrame::Owned(
            NDIlib_metadata_frame_t {
                length: data
                    .as_ref()
                    .map(|s| s.to_str().unwrap().len())
                    .unwrap_or(0) as i32,
                timecode,
                p_data: data
                    .as_ref()
                    .map(|s| s.as_ptr() as *mut _)
                    .unwrap_or(ptr::null_mut()),
            },
            data,
        )
    }

    pub fn timecode(&self) -> i64 {
        match self {
            MetadataFrame::Owned(ref frame, _) => frame.timecode,
            MetadataFrame::Borrowed(ref frame, _) => frame.timecode,
        }
    }

    pub fn metadata(&self) -> Option<&str> {
        unsafe {
            match self {
                MetadataFrame::Owned(_, ref metadata) => {
                    metadata.as_ref().map(|s| s.to_str().unwrap())
                }
                MetadataFrame::Borrowed(ref frame, _) => {
                    if frame.p_data.is_null() || frame.length == 0 {
                        None
                    } else if frame.length != 0 {
                        use std::slice;

                        Some(
                            ffi::CStr::from_bytes_with_nul_unchecked(slice::from_raw_parts(
                                frame.p_data as *const u8,
                                frame.length as usize,
                            ))
                            .to_str()
                            .unwrap(),
                        )
                    } else {
                        Some(ffi::CStr::from_ptr(frame.p_data).to_str().unwrap())
                    }
                }
            }
        }
    }

    pub fn as_ptr(&self) -> *const NDIlib_metadata_frame_t {
        match self {
            MetadataFrame::Owned(ref frame, _) => frame,
            MetadataFrame::Borrowed(ref frame, _) => frame,
        }
    }
}

impl<'a> Default for MetadataFrame<'a> {
    fn default() -> Self {
        MetadataFrame::Owned(
            NDIlib_metadata_frame_t {
                length: 0,
                timecode: 0, //NDIlib_send_timecode_synthesize,
                p_data: ptr::null(),
            },
            None,
        )
    }
}

impl<'a> Drop for MetadataFrame<'a> {
    fn drop(&mut self) {
        if let MetadataFrame::Borrowed(ref mut frame, ref recv) = *self {
            unsafe {
                NDIlib_recv_free_metadata(((recv.0).0).0.as_ptr() as *mut _, frame);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Queue(NDIlib_recv_queue_t);

impl Queue {
    pub fn audio_frames(&self) -> i32 {
        self.0.audio_frames
    }
    pub fn video_frames(&self) -> i32 {
        self.0.video_frames
    }
    pub fn metadata_frames(&self) -> i32 {
        self.0.metadata_frames
    }
}
