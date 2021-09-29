use crate::ndisys;
use crate::ndisys::*;
use std::ffi;
use std::mem;
use std::ptr;

use byte_slice_cast::*;

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
unsafe impl<'a> Sync for Source<'a> {}

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

impl<'a> PartialEq for Source<'a> {
    fn eq(&self, other: &Source<'a>) -> bool {
        self.ndi_name() == other.ndi_name() && self.url_address() == other.url_address()
    }
}

#[derive(Debug)]
pub struct RecvBuilder<'a> {
    ndi_name: Option<&'a str>,
    url_address: Option<&'a str>,
    allow_video_fields: bool,
    bandwidth: NDIlib_recv_bandwidth_e,
    color_format: NDIlib_recv_color_format_e,
    ndi_recv_name: &'a str,
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
            let ndi_recv_name = ffi::CString::new(self.ndi_recv_name).unwrap();
            let ndi_name = self
                .ndi_name
                .as_ref()
                .map(|s| ffi::CString::new(*s).unwrap());
            let url_address = self
                .url_address
                .as_ref()
                .map(|s| ffi::CString::new(*s).unwrap());
            let ptr = NDIlib_recv_create_v3(&NDIlib_recv_create_v3_t {
                source_to_connect_to: NDIlib_source_t {
                    p_ndi_name: ndi_name
                        .as_ref()
                        .map(|s| s.as_ptr())
                        .unwrap_or_else(|| ptr::null_mut()),
                    p_url_address: url_address
                        .as_ref()
                        .map(|s| s.as_ptr())
                        .unwrap_or_else(|| ptr::null_mut()),
                },
                allow_video_fields: self.allow_video_fields,
                bandwidth: self.bandwidth,
                color_format: self.color_format,
                p_ndi_recv_name: ndi_recv_name.as_ptr(),
            });

            if ptr.is_null() {
                None
            } else {
                Some(RecvInstance(ptr::NonNull::new_unchecked(ptr)))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecvInstance(ptr::NonNull<::std::os::raw::c_void>);

unsafe impl Send for RecvInstance {}

impl RecvInstance {
    pub fn builder<'a>(
        ndi_name: Option<&'a str>,
        url_address: Option<&'a str>,
        ndi_recv_name: &'a str,
    ) -> RecvBuilder<'a> {
        RecvBuilder {
            ndi_name,
            url_address,
            allow_video_fields: true,
            bandwidth: NDIlib_recv_bandwidth_highest,
            color_format: NDIlib_recv_color_format_e::NDIlib_recv_color_format_UYVY_BGRA,
            ndi_recv_name,
        }
    }

    pub fn set_tally(&self, tally: &Tally) -> bool {
        unsafe { NDIlib_recv_set_tally(self.0.as_ptr(), &tally.0) }
    }

    pub fn send_metadata(&self, metadata: &MetadataFrame) -> bool {
        unsafe { NDIlib_recv_send_metadata(self.0.as_ptr(), metadata.as_ptr()) }
    }

    pub fn get_queue(&self) -> Queue {
        unsafe {
            let mut queue = mem::MaybeUninit::uninit();
            NDIlib_recv_get_queue(self.0.as_ptr(), queue.as_mut_ptr());
            Queue(queue.assume_init())
        }
    }

    pub fn capture(&self, timeout_in_ms: u32) -> Result<Option<Frame>, ()> {
        unsafe {
            let ptr = self.0.as_ptr();

            let mut video_frame = mem::zeroed();
            let mut audio_frame = mem::zeroed();
            let mut metadata_frame = mem::zeroed();

            let res = NDIlib_recv_capture_v2(
                ptr,
                &mut video_frame,
                &mut audio_frame,
                &mut metadata_frame,
                timeout_in_ms,
            );

            match res {
                NDIlib_frame_type_e::NDIlib_frame_type_audio => Ok(Some(Frame::Audio(
                    AudioFrame::BorrowedRecv(audio_frame, self),
                ))),
                NDIlib_frame_type_e::NDIlib_frame_type_video => Ok(Some(Frame::Video(
                    VideoFrame::BorrowedRecv(video_frame, self),
                ))),
                NDIlib_frame_type_e::NDIlib_frame_type_metadata => Ok(Some(Frame::Metadata(
                    MetadataFrame::Borrowed(metadata_frame, self),
                ))),
                NDIlib_frame_type_e::NDIlib_frame_type_error => Err(()),
                _ => Ok(None),
            }
        }
    }
}

impl Drop for RecvInstance {
    fn drop(&mut self) {
        unsafe { NDIlib_recv_destroy(self.0.as_ptr() as *mut _) }
    }
}

#[derive(Debug)]
pub struct SendBuilder<'a> {
    ndi_name: &'a str,
    clock_audio: bool,
    clock_video: bool,
}

impl<'a> SendBuilder<'a> {
    pub fn clock_audio(self) -> Self {
        Self {
            clock_audio: true,
            ..self
        }
    }

    pub fn clock_video(self) -> Self {
        Self {
            clock_video: true,
            ..self
        }
    }

    pub fn build(self) -> Option<SendInstance> {
        unsafe {
            let ndi_name = ffi::CString::new(self.ndi_name).unwrap();
            let ptr = NDIlib_send_create(&NDIlib_send_create_t {
                p_ndi_name: ndi_name.as_ptr(),
                clock_video: self.clock_video,
                clock_audio: self.clock_audio,
                p_groups: ptr::null(),
            });

            if ptr.is_null() {
                None
            } else {
                Some(SendInstance(ptr::NonNull::new_unchecked(ptr)))
            }
        }
    }
}

#[derive(Debug)]
pub struct SendInstance(ptr::NonNull<::std::os::raw::c_void>);

unsafe impl Send for SendInstance {}

impl SendInstance {
    pub fn builder(ndi_name: &str) -> SendBuilder {
        SendBuilder {
            ndi_name,
            clock_video: false,
            clock_audio: false,
        }
    }

    pub fn send_video(&mut self, frame: &VideoFrame) {
        unsafe {
            NDIlib_send_send_video_v2(self.0.as_ptr(), frame.as_ptr());
        }
    }

    pub fn send_audio(&mut self, frame: &AudioFrame) {
        unsafe {
            NDIlib_send_send_audio_v2(self.0.as_ptr(), frame.as_ptr());
        }
    }
}

impl Drop for SendInstance {
    fn drop(&mut self) {
        unsafe { NDIlib_send_destroy(self.0.as_ptr() as *mut _) }
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
    BorrowedRecv(NDIlib_video_frame_v2_t, &'a RecvInstance),
    BorrowedGst(
        NDIlib_video_frame_v2_t,
        &'a gst_video::VideoFrameRef<&'a gst::BufferRef>,
    ),
}

impl<'a> VideoFrame<'a> {
    pub fn xres(&self) -> i32 {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                frame.xres
            }
        }
    }

    pub fn yres(&self) -> i32 {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                frame.yres
            }
        }
    }

    pub fn fourcc(&self) -> NDIlib_FourCC_video_type_e {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                frame.FourCC
            }
        }
    }

    pub fn frame_rate(&self) -> (i32, i32) {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                (frame.frame_rate_N, frame.frame_rate_D)
            }
        }
    }

    pub fn picture_aspect_ratio(&self) -> f32 {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                frame.picture_aspect_ratio
            }
        }
    }

    pub fn frame_format_type(&self) -> NDIlib_frame_format_type_e {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                frame.frame_format_type
            }
        }
    }

    pub fn timecode(&self) -> i64 {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                frame.timecode
            }
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
                VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                    slice::from_raw_parts(frame.p_data as *const u8, frame_size as usize)
                }
            }
        }
    }

    pub fn line_stride_or_data_size_in_bytes(&self) -> i32 {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
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
                VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
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
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => {
                frame.timestamp
            }
        }
    }

    pub fn as_ptr(&self) -> *const NDIlib_video_frame_v2_t {
        match self {
            VideoFrame::BorrowedRecv(ref frame, _) | VideoFrame::BorrowedGst(ref frame, _) => frame,
        }
    }

    pub fn try_from_video_frame(
        frame: &'a gst_video::VideoFrameRef<&'a gst::BufferRef>,
        timecode: i64,
    ) -> Result<Self, ()> {
        // Planar formats must be in contiguous memory
        let format = match frame.format() {
            gst_video::VideoFormat::Uyvy => ndisys::NDIlib_FourCC_video_type_UYVY,
            gst_video::VideoFormat::I420 => {
                if (frame.plane_data(1).unwrap().as_ptr() as usize)
                    .checked_sub(frame.plane_data(0).unwrap().as_ptr() as usize)
                    != Some(frame.height() as usize * frame.plane_stride()[0] as usize)
                {
                    return Err(());
                }

                if (frame.plane_data(2).unwrap().as_ptr() as usize)
                    .checked_sub(frame.plane_data(1).unwrap().as_ptr() as usize)
                    != Some((frame.height() as usize + 1) / 2 * frame.plane_stride()[1] as usize)
                {
                    return Err(());
                }

                ndisys::NDIlib_FourCC_video_type_I420
            }
            gst_video::VideoFormat::Nv12 => {
                if (frame.plane_data(1).unwrap().as_ptr() as usize)
                    .checked_sub(frame.plane_data(0).unwrap().as_ptr() as usize)
                    != Some(frame.height() as usize * frame.plane_stride()[0] as usize)
                {
                    return Err(());
                }

                ndisys::NDIlib_FourCC_video_type_NV12
            }
            gst_video::VideoFormat::Nv21 => {
                if (frame.plane_data(1).unwrap().as_ptr() as usize)
                    .checked_sub(frame.plane_data(0).unwrap().as_ptr() as usize)
                    != Some(frame.height() as usize * frame.plane_stride()[0] as usize)
                {
                    return Err(());
                }

                ndisys::NDIlib_FourCC_video_type_NV12
            }
            gst_video::VideoFormat::Yv12 => {
                if (frame.plane_data(1).unwrap().as_ptr() as usize)
                    .checked_sub(frame.plane_data(0).unwrap().as_ptr() as usize)
                    != Some(frame.height() as usize * frame.plane_stride()[0] as usize)
                {
                    return Err(());
                }

                if (frame.plane_data(2).unwrap().as_ptr() as usize)
                    .checked_sub(frame.plane_data(1).unwrap().as_ptr() as usize)
                    != Some((frame.height() as usize + 1) / 2 * frame.plane_stride()[1] as usize)
                {
                    return Err(());
                }

                ndisys::NDIlib_FourCC_video_type_YV12
            }
            gst_video::VideoFormat::Bgra => ndisys::NDIlib_FourCC_video_type_BGRA,
            gst_video::VideoFormat::Bgrx => ndisys::NDIlib_FourCC_video_type_BGRX,
            gst_video::VideoFormat::Rgba => ndisys::NDIlib_FourCC_video_type_RGBA,
            gst_video::VideoFormat::Rgbx => ndisys::NDIlib_FourCC_video_type_RGBX,
            _ => return Err(()),
        };

        let frame_format_type = match frame.info().interlace_mode() {
            gst_video::VideoInterlaceMode::Progressive => {
                NDIlib_frame_format_type_e::NDIlib_frame_format_type_progressive
            }
            gst_video::VideoInterlaceMode::Interleaved => {
                NDIlib_frame_format_type_e::NDIlib_frame_format_type_interleaved
            }
            // FIXME: Is this correct?
            #[cfg(feature = "interlaced-fields")]
            gst_video::VideoInterlaceMode::Alternate
                if frame.flags().contains(gst_video::VideoFrameFlags::TFF) =>
            {
                NDIlib_frame_format_type_e::NDIlib_frame_format_type_field_0
            }
            #[cfg(feature = "interlaced-fields")]
            gst_video::VideoInterlaceMode::Alternate
                if !frame.flags().contains(gst_video::VideoFrameFlags::TFF) =>
            {
                NDIlib_frame_format_type_e::NDIlib_frame_format_type_field_1
            }
            _ => return Err(()),
        };

        let picture_aspect_ratio =
            frame.info().par() * gst::Fraction::new(frame.width() as i32, frame.height() as i32);
        let picture_aspect_ratio =
            *picture_aspect_ratio.numer() as f32 / *picture_aspect_ratio.denom() as f32;

        let ndi_frame = NDIlib_video_frame_v2_t {
            xres: frame.width() as i32,
            yres: frame.height() as i32,
            FourCC: format,
            frame_rate_N: *frame.info().fps().numer(),
            frame_rate_D: *frame.info().fps().denom(),
            picture_aspect_ratio,
            frame_format_type,
            timecode,
            p_data: frame.plane_data(0).unwrap().as_ptr() as *const i8,
            line_stride_or_data_size_in_bytes: frame.plane_stride()[0],
            p_metadata: ptr::null(),
            timestamp: 0,
        };

        Ok(VideoFrame::BorrowedGst(ndi_frame, frame))
    }
}

impl<'a> Drop for VideoFrame<'a> {
    #[allow(irrefutable_let_patterns)]
    fn drop(&mut self) {
        if let VideoFrame::BorrowedRecv(ref mut frame, recv) = *self {
            unsafe {
                NDIlib_recv_free_video_v2(recv.0.as_ptr() as *mut _, frame);
            }
        }
    }
}

#[derive(Debug)]
pub enum AudioFrame<'a> {
    Owned(
        NDIlib_audio_frame_v2_t,
        Option<ffi::CString>,
        Option<Vec<f32>>,
    ),
    BorrowedRecv(NDIlib_audio_frame_v2_t, &'a RecvInstance),
}

impl<'a> AudioFrame<'a> {
    pub fn sample_rate(&self) -> i32 {
        match self {
            AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => {
                frame.sample_rate
            }
        }
    }

    pub fn no_channels(&self) -> i32 {
        match self {
            AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => {
                frame.no_channels
            }
        }
    }

    pub fn no_samples(&self) -> i32 {
        match self {
            AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => {
                frame.no_samples
            }
        }
    }

    pub fn timecode(&self) -> i64 {
        match self {
            AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => {
                frame.timecode
            }
        }
    }

    pub fn data(&self) -> &[u8] {
        unsafe {
            use std::slice;
            match self {
                AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => {
                    slice::from_raw_parts(
                        frame.p_data as *const u8,
                        (frame.no_samples * frame.channel_stride_in_bytes) as usize,
                    )
                }
            }
        }
    }

    pub fn channel_stride_in_bytes(&self) -> i32 {
        match self {
            AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => {
                frame.channel_stride_in_bytes
            }
        }
    }

    pub fn metadata(&self) -> Option<&str> {
        unsafe {
            match self {
                AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => {
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
            AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => {
                frame.timestamp
            }
        }
    }

    pub fn as_ptr(&self) -> *const NDIlib_audio_frame_v2_t {
        match self {
            AudioFrame::BorrowedRecv(ref frame, _) | AudioFrame::Owned(ref frame, _, _) => frame,
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

    pub fn try_from_interleaved_16s(
        info: &gst_audio::AudioInfo,
        buffer: &gst::BufferRef,
        timecode: i64,
    ) -> Result<Self, ()> {
        if info.format() != gst_audio::AUDIO_FORMAT_S16 {
            return Err(());
        }

        let map = buffer.map_readable().map_err(|_| ())?;
        let src_data = map.as_slice_of::<i16>().map_err(|_| ())?;

        let src = NDIlib_audio_frame_interleaved_16s_t {
            sample_rate: info.rate() as i32,
            no_channels: info.channels() as i32,
            no_samples: src_data.len() as i32 / info.channels() as i32,
            timecode,
            reference_level: 0,
            p_data: src_data.as_ptr() as *mut i16,
        };

        let channel_stride_in_bytes = src.no_samples * mem::size_of::<f32>() as i32;
        let mut dest_data =
            Vec::with_capacity(channel_stride_in_bytes as usize * info.channels() as usize);

        let mut dest = NDIlib_audio_frame_v2_t {
            sample_rate: src.sample_rate,
            no_channels: src.no_channels,
            no_samples: src.no_samples,
            timecode: src.timecode,
            p_data: dest_data.as_mut_ptr(),
            channel_stride_in_bytes,
            p_metadata: ptr::null(),
            timestamp: 0,
        };

        unsafe {
            NDIlib_util_audio_from_interleaved_16s_v2(&src, &mut dest);
            dest_data.set_len(dest_data.capacity());
        }

        Ok(AudioFrame::Owned(dest, None, Some(dest_data)))
    }
}

impl<'a> Drop for AudioFrame<'a> {
    #[allow(irrefutable_let_patterns)]
    fn drop(&mut self) {
        if let AudioFrame::BorrowedRecv(ref mut frame, recv) = *self {
            unsafe {
                NDIlib_recv_free_audio_v2(recv.0.as_ptr() as *mut _, frame);
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
        if let MetadataFrame::Borrowed(ref mut frame, recv) = *self {
            unsafe {
                NDIlib_recv_free_metadata(recv.0.as_ptr() as *mut _, frame);
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
