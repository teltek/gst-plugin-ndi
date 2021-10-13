// Take a look at the license at the top of the repository in the LICENSE file.

use super::ffi;
use super::Aggregator;

use glib::signal::{connect_raw, SignalHandlerId};
use glib::translate::*;
use glib::IsA;
use glib::Value;
use gst::glib;
use gst::prelude::*;

use std::boxed::Box as Box_;
use std::mem;
use std::ptr;

pub trait AggregatorExtManual: 'static {
    fn allocator(&self) -> (Option<gst::Allocator>, gst::AllocationParams);

    fn finish_buffer(&self, buffer: gst::Buffer) -> Result<gst::FlowSuccess, gst::FlowError>;
    fn min_upstream_latency(&self) -> gst::ClockTime;

    fn set_min_upstream_latency(&self, min_upstream_latency: gst::ClockTime);

    #[doc(alias = "min-upstream-latency")]
    fn connect_min_upstream_latency_notify<F: Fn(&Self) + Send + Sync + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId;
}

impl<O: IsA<Aggregator>> AggregatorExtManual for O {
    fn allocator(&self) -> (Option<gst::Allocator>, gst::AllocationParams) {
        unsafe {
            let mut allocator = ptr::null_mut();
            let mut params = mem::zeroed();
            ffi::gst_aggregator_get_allocator(
                self.as_ref().to_glib_none().0,
                &mut allocator,
                &mut params,
            );
            (from_glib_full(allocator), params.into())
        }
    }

    fn finish_buffer(&self, buffer: gst::Buffer) -> Result<gst::FlowSuccess, gst::FlowError> {
        unsafe {
            try_from_glib(ffi::gst_aggregator_finish_buffer(
                self.as_ref().to_glib_none().0,
                buffer.into_ptr(),
            ))
        }
    }

    fn min_upstream_latency(&self) -> gst::ClockTime {
        unsafe {
            let mut value = Value::from_type(<gst::ClockTime as StaticType>::static_type());
            glib::gobject_ffi::g_object_get_property(
                self.to_glib_none().0 as *mut glib::gobject_ffi::GObject,
                b"min-upstream-latency\0".as_ptr() as *const _,
                value.to_glib_none_mut().0,
            );
            value
                .get()
                .expect("AggregatorExtManual::min_upstream_latency")
        }
    }

    fn set_min_upstream_latency(&self, min_upstream_latency: gst::ClockTime) {
        unsafe {
            glib::gobject_ffi::g_object_set_property(
                self.to_glib_none().0 as *mut glib::gobject_ffi::GObject,
                b"min-upstream-latency\0".as_ptr() as *const _,
                Value::from(&min_upstream_latency).to_glib_none().0,
            );
        }
    }

    fn connect_min_upstream_latency_notify<F: Fn(&Self) + Send + Sync + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        unsafe {
            let f: Box_<F> = Box_::new(f);
            connect_raw(
                self.as_ptr() as *mut _,
                b"notify::min-upstream-latency\0".as_ptr() as *const _,
                Some(mem::transmute::<_, unsafe extern "C" fn()>(
                    notify_min_upstream_latency_trampoline::<Self, F> as *const (),
                )),
                Box_::into_raw(f),
            )
        }
    }
}

unsafe extern "C" fn notify_min_upstream_latency_trampoline<P, F: Fn(&P) + Send + Sync + 'static>(
    this: *mut ffi::GstAggregator,
    _param_spec: glib::ffi::gpointer,
    f: glib::ffi::gpointer,
) where
    P: IsA<Aggregator>,
{
    let f: &F = &*(f as *const F);
    f(&Aggregator::from_glib_borrow(this).unsafe_cast_ref())
}
