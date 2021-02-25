use gst::gst_sys;
use gst::prelude::*;
use std::fmt;
use std::mem;

#[repr(transparent)]
pub struct NdiSinkAudioMeta(imp::NdiSinkAudioMeta);

unsafe impl Send for NdiSinkAudioMeta {}
unsafe impl Sync for NdiSinkAudioMeta {}

impl NdiSinkAudioMeta {
    pub fn add(
        buffer: &mut gst::BufferRef,
        buffers: Vec<(gst::Buffer, gst_audio::AudioInfo, i64)>,
    ) -> gst::MetaRefMut<Self, gst::meta::Standalone> {
        unsafe {
            // Manually dropping because gst_buffer_add_meta() takes ownership of the
            // content of the struct
            let mut params = mem::ManuallyDrop::new(imp::NdiSinkAudioMetaParams { buffers });

            let meta = gst_sys::gst_buffer_add_meta(
                buffer.as_mut_ptr(),
                imp::ndi_sink_audio_meta_get_info(),
                &mut *params as *mut imp::NdiSinkAudioMetaParams as glib::glib_sys::gpointer,
            ) as *mut imp::NdiSinkAudioMeta;

            Self::from_mut_ptr(buffer, meta)
        }
    }

    pub fn buffers(&self) -> &[(gst::Buffer, gst_audio::AudioInfo, i64)] {
        &self.0.buffers
    }
}

unsafe impl MetaAPI for NdiSinkAudioMeta {
    type GstType = imp::NdiSinkAudioMeta;

    fn get_meta_api() -> glib::Type {
        imp::ndi_sink_audio_meta_api_get_type()
    }
}

impl fmt::Debug for NdiSinkAudioMeta {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("NdiSinkAudioMeta")
            .field("buffers", &self.buffers())
            .finish()
    }
}

mod imp {
    use glib::glib_sys;
    use glib::translate::*;
    use gst::gst_sys;
    use once_cell::sync::Lazy;
    use std::mem;
    use std::ptr;

    pub(super) struct NdiSinkAudioMetaParams {
        pub buffers: Vec<(gst::Buffer, gst_audio::AudioInfo, i64)>,
    }

    #[repr(C)]
    pub struct NdiSinkAudioMeta {
        parent: gst_sys::GstMeta,
        pub(super) buffers: Vec<(gst::Buffer, gst_audio::AudioInfo, i64)>,
    }

    pub(super) fn ndi_sink_audio_meta_api_get_type() -> glib::Type {
        static TYPE: Lazy<glib::Type> = Lazy::new(|| unsafe {
            let t = from_glib(gst_sys::gst_meta_api_type_register(
                b"GstNdiSinkAudioMetaAPI\0".as_ptr() as *const _,
                [ptr::null::<std::os::raw::c_char>()].as_ptr() as *mut *const _,
            ));

            assert_ne!(t, glib::Type::Invalid);

            t
        });

        *TYPE
    }

    unsafe extern "C" fn ndi_sink_audio_meta_init(
        meta: *mut gst_sys::GstMeta,
        params: glib_sys::gpointer,
        _buffer: *mut gst_sys::GstBuffer,
    ) -> glib_sys::gboolean {
        assert!(!params.is_null());

        let meta = &mut *(meta as *mut NdiSinkAudioMeta);
        let params = ptr::read(params as *const NdiSinkAudioMetaParams);

        ptr::write(&mut meta.buffers, params.buffers);

        true.to_glib()
    }

    unsafe extern "C" fn ndi_sink_audio_meta_free(
        meta: *mut gst_sys::GstMeta,
        _buffer: *mut gst_sys::GstBuffer,
    ) {
        let meta = &mut *(meta as *mut NdiSinkAudioMeta);

        ptr::drop_in_place(&mut meta.buffers);
    }

    unsafe extern "C" fn ndi_sink_audio_meta_transform(
        dest: *mut gst_sys::GstBuffer,
        meta: *mut gst_sys::GstMeta,
        _buffer: *mut gst_sys::GstBuffer,
        _type_: glib_sys::GQuark,
        _data: glib_sys::gpointer,
    ) -> glib_sys::gboolean {
        let meta = &*(meta as *mut NdiSinkAudioMeta);

        super::NdiSinkAudioMeta::add(gst::BufferRef::from_mut_ptr(dest), meta.buffers.clone());

        true.to_glib()
    }

    pub(super) fn ndi_sink_audio_meta_get_info() -> *const gst_sys::GstMetaInfo {
        struct MetaInfo(ptr::NonNull<gst_sys::GstMetaInfo>);
        unsafe impl Send for MetaInfo {}
        unsafe impl Sync for MetaInfo {}

        static META_INFO: Lazy<MetaInfo> = Lazy::new(|| unsafe {
            MetaInfo(
                ptr::NonNull::new(gst_sys::gst_meta_register(
                    ndi_sink_audio_meta_api_get_type().to_glib(),
                    b"GstNdiSinkAudioMeta\0".as_ptr() as *const _,
                    mem::size_of::<NdiSinkAudioMeta>(),
                    Some(ndi_sink_audio_meta_init),
                    Some(ndi_sink_audio_meta_free),
                    Some(ndi_sink_audio_meta_transform),
                ) as *mut gst_sys::GstMetaInfo)
                .expect("Failed to register meta API"),
            )
        });

        META_INFO.0.as_ptr()
    }
}
