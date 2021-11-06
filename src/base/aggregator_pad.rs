// Take a look at the license at the top of the repository in the LICENSE file.

use super::ffi;

use super::AggregatorPad;
use glib::prelude::*;
use glib::translate::*;

pub trait AggregatorPadExtManual: 'static {
    #[doc(alias = "get_segment")]
    fn segment(&self) -> gst::Segment;
}

impl<O: IsA<AggregatorPad>> AggregatorPadExtManual for O {
    fn segment(&self) -> gst::Segment {
        unsafe {
            let ptr: &ffi::GstAggregatorPad = &*(self.as_ptr() as *const _);
            let _guard = super::utils::MutexGuard::lock(&ptr.parent.object.lock);
            from_glib_none(&ptr.segment as *const gst::ffi::GstSegment)
        }
    }
}
