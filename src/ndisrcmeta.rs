use gst::prelude::*;
use std::fmt;
use std::mem;

#[repr(transparent)]
pub struct NdiSrcMeta(imp::NdiSrcMeta);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StreamType {
    Audio,
    Video,
}

unsafe impl Send for NdiSrcMeta {}
unsafe impl Sync for NdiSrcMeta {}

impl NdiSrcMeta {
    pub fn add<'a>(
        buffer: &'a mut gst::BufferRef,
        stream_type: StreamType,
        caps: &gst::Caps,
    ) -> gst::MetaRefMut<'a, Self, gst::meta::Standalone> {
        unsafe {
            // Manually dropping because gst_buffer_add_meta() takes ownership of the
            // content of the struct
            let mut params = mem::ManuallyDrop::new(imp::NdiSrcMetaParams {
                caps: caps.clone(),
                stream_type,
            });

            let meta = gst::ffi::gst_buffer_add_meta(
                buffer.as_mut_ptr(),
                imp::ndi_src_meta_get_info(),
                &mut *params as *mut imp::NdiSrcMetaParams as glib::ffi::gpointer,
            ) as *mut imp::NdiSrcMeta;

            Self::from_mut_ptr(buffer, meta)
        }
    }

    pub fn stream_type(&self) -> StreamType {
        self.0.stream_type
    }

    pub fn caps(&self) -> gst::Caps {
        self.0.caps.clone()
    }
}

unsafe impl MetaAPI for NdiSrcMeta {
    type GstType = imp::NdiSrcMeta;

    fn meta_api() -> glib::Type {
        imp::ndi_src_meta_api_get_type()
    }
}

impl fmt::Debug for NdiSrcMeta {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("NdiSrcMeta")
            .field("stream_type", &self.stream_type())
            .field("caps", &self.caps())
            .finish()
    }
}

mod imp {
    use super::StreamType;
    use glib::translate::*;
    use once_cell::sync::Lazy;
    use std::mem;
    use std::ptr;

    pub(super) struct NdiSrcMetaParams {
        pub caps: gst::Caps,
        pub stream_type: StreamType,
    }

    #[repr(C)]
    pub struct NdiSrcMeta {
        parent: gst::ffi::GstMeta,
        pub(super) caps: gst::Caps,
        pub(super) stream_type: StreamType,
    }

    pub(super) fn ndi_src_meta_api_get_type() -> glib::Type {
        static TYPE: Lazy<glib::Type> = Lazy::new(|| unsafe {
            let t = from_glib(gst::ffi::gst_meta_api_type_register(
                b"GstNdiSrcMetaAPI\0".as_ptr() as *const _,
                [ptr::null::<std::os::raw::c_char>()].as_ptr() as *mut *const _,
            ));

            assert_ne!(t, glib::Type::INVALID);

            t
        });

        *TYPE
    }

    unsafe extern "C" fn ndi_src_meta_init(
        meta: *mut gst::ffi::GstMeta,
        params: glib::ffi::gpointer,
        _buffer: *mut gst::ffi::GstBuffer,
    ) -> glib::ffi::gboolean {
        assert!(!params.is_null());

        let meta = &mut *(meta as *mut NdiSrcMeta);
        let params = ptr::read(params as *const NdiSrcMetaParams);

        ptr::write(&mut meta.stream_type, params.stream_type);
        ptr::write(&mut meta.caps, params.caps);

        true.into_glib()
    }

    unsafe extern "C" fn ndi_src_meta_free(
        meta: *mut gst::ffi::GstMeta,
        _buffer: *mut gst::ffi::GstBuffer,
    ) {
        let meta = &mut *(meta as *mut NdiSrcMeta);

        ptr::drop_in_place(&mut meta.stream_type);
        ptr::drop_in_place(&mut meta.caps);
    }

    unsafe extern "C" fn ndi_src_meta_transform(
        _dest: *mut gst::ffi::GstBuffer,
        _meta: *mut gst::ffi::GstMeta,
        _buffer: *mut gst::ffi::GstBuffer,
        _type_: glib::ffi::GQuark,
        _data: glib::ffi::gpointer,
    ) -> glib::ffi::gboolean {
        false.into_glib()
    }

    pub(super) fn ndi_src_meta_get_info() -> *const gst::ffi::GstMetaInfo {
        struct MetaInfo(ptr::NonNull<gst::ffi::GstMetaInfo>);
        unsafe impl Send for MetaInfo {}
        unsafe impl Sync for MetaInfo {}

        static META_INFO: Lazy<MetaInfo> = Lazy::new(|| unsafe {
            MetaInfo(
                ptr::NonNull::new(gst::ffi::gst_meta_register(
                    ndi_src_meta_api_get_type().into_glib(),
                    b"GstNdiSrcMeta\0".as_ptr() as *const _,
                    mem::size_of::<NdiSrcMeta>(),
                    Some(ndi_src_meta_init),
                    Some(ndi_src_meta_free),
                    Some(ndi_src_meta_transform),
                ) as *mut gst::ffi::GstMetaInfo)
                .expect("Failed to register meta API"),
            )
        });

        META_INFO.0.as_ptr()
    }
}
