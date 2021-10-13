#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]
#![allow(
    clippy::approx_constant,
    clippy::type_complexity,
    clippy::unreadable_literal
)]

use gst::ffi as gst;

#[allow(unused_imports)]
use libc::{
    c_char, c_double, c_float, c_int, c_long, c_short, c_uchar, c_uint, c_ulong, c_ushort, c_void,
    intptr_t, size_t, ssize_t, time_t, uintptr_t, FILE,
};

#[allow(unused_imports)]
use ::gst::glib::ffi::{gboolean, gconstpointer, gpointer, GType};

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GstAggregatorClass {
    pub parent_class: gst::GstElementClass,
    pub flush: Option<unsafe extern "C" fn(*mut GstAggregator) -> gst::GstFlowReturn>,
    pub clip: Option<
        unsafe extern "C" fn(
            *mut GstAggregator,
            *mut GstAggregatorPad,
            *mut gst::GstBuffer,
        ) -> *mut gst::GstBuffer,
    >,
    pub finish_buffer:
        Option<unsafe extern "C" fn(*mut GstAggregator, *mut gst::GstBuffer) -> gst::GstFlowReturn>,
    pub sink_event: Option<
        unsafe extern "C" fn(
            *mut GstAggregator,
            *mut GstAggregatorPad,
            *mut gst::GstEvent,
        ) -> gboolean,
    >,
    pub sink_query: Option<
        unsafe extern "C" fn(
            *mut GstAggregator,
            *mut GstAggregatorPad,
            *mut gst::GstQuery,
        ) -> gboolean,
    >,
    pub src_event: Option<unsafe extern "C" fn(*mut GstAggregator, *mut gst::GstEvent) -> gboolean>,
    pub src_query: Option<unsafe extern "C" fn(*mut GstAggregator, *mut gst::GstQuery) -> gboolean>,
    pub src_activate:
        Option<unsafe extern "C" fn(*mut GstAggregator, gst::GstPadMode, gboolean) -> gboolean>,
    pub aggregate: Option<unsafe extern "C" fn(*mut GstAggregator, gboolean) -> gst::GstFlowReturn>,
    pub stop: Option<unsafe extern "C" fn(*mut GstAggregator) -> gboolean>,
    pub start: Option<unsafe extern "C" fn(*mut GstAggregator) -> gboolean>,
    pub get_next_time: Option<unsafe extern "C" fn(*mut GstAggregator) -> gst::GstClockTime>,
    pub create_new_pad: Option<
        unsafe extern "C" fn(
            *mut GstAggregator,
            *mut gst::GstPadTemplate,
            *const c_char,
            *const gst::GstCaps,
        ) -> *mut GstAggregatorPad,
    >,
    pub update_src_caps: Option<
        unsafe extern "C" fn(
            *mut GstAggregator,
            *mut gst::GstCaps,
            *mut *mut gst::GstCaps,
        ) -> gst::GstFlowReturn,
    >,
    pub fixate_src_caps:
        Option<unsafe extern "C" fn(*mut GstAggregator, *mut gst::GstCaps) -> *mut gst::GstCaps>,
    pub negotiated_src_caps:
        Option<unsafe extern "C" fn(*mut GstAggregator, *mut gst::GstCaps) -> gboolean>,
    pub decide_allocation:
        Option<unsafe extern "C" fn(*mut GstAggregator, *mut gst::GstQuery) -> gboolean>,
    pub propose_allocation: Option<
        unsafe extern "C" fn(
            *mut GstAggregator,
            *mut GstAggregatorPad,
            *mut gst::GstQuery,
            *mut gst::GstQuery,
        ) -> gboolean,
    >,
    pub negotiate: Option<unsafe extern "C" fn(*mut GstAggregator) -> gboolean>,
    pub sink_event_pre_queue: Option<
        unsafe extern "C" fn(
            *mut GstAggregator,
            *mut GstAggregatorPad,
            *mut gst::GstEvent,
        ) -> gboolean,
    >,
    pub sink_query_pre_queue: Option<
        unsafe extern "C" fn(
            *mut GstAggregator,
            *mut GstAggregatorPad,
            *mut gst::GstQuery,
        ) -> gboolean,
    >,
    pub _gst_reserved: [gpointer; 17],
}

impl ::std::fmt::Debug for GstAggregatorClass {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct(&format!("GstAggregatorClass @ {:?}", self as *const _))
            .field("parent_class", &self.parent_class)
            .field("flush", &self.flush)
            .field("clip", &self.clip)
            .field("finish_buffer", &self.finish_buffer)
            .field("sink_event", &self.sink_event)
            .field("sink_query", &self.sink_query)
            .field("src_event", &self.src_event)
            .field("src_query", &self.src_query)
            .field("src_activate", &self.src_activate)
            .field("aggregate", &self.aggregate)
            .field("stop", &self.stop)
            .field("start", &self.start)
            .field("get_next_time", &self.get_next_time)
            .field("create_new_pad", &self.create_new_pad)
            .field("update_src_caps", &self.update_src_caps)
            .field("fixate_src_caps", &self.fixate_src_caps)
            .field("negotiated_src_caps", &self.negotiated_src_caps)
            .field("decide_allocation", &self.decide_allocation)
            .field("propose_allocation", &self.propose_allocation)
            .finish()
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GstAggregatorPadClass {
    pub parent_class: gst::GstPadClass,
    pub flush: Option<
        unsafe extern "C" fn(*mut GstAggregatorPad, *mut GstAggregator) -> gst::GstFlowReturn,
    >,
    pub skip_buffer: Option<
        unsafe extern "C" fn(
            *mut GstAggregatorPad,
            *mut GstAggregator,
            *mut gst::GstBuffer,
        ) -> gboolean,
    >,
    pub _gst_reserved: [gpointer; 20],
}

impl ::std::fmt::Debug for GstAggregatorPadClass {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct(&format!("GstAggregatorPadClass @ {:?}", self as *const _))
            .field("parent_class", &self.parent_class)
            .field("flush", &self.flush)
            .field("skip_buffer", &self.skip_buffer)
            .finish()
    }
}

#[repr(C)]
pub struct _GstAggregatorPadPrivate(c_void);

pub type GstAggregatorPadPrivate = *mut _GstAggregatorPadPrivate;

#[repr(C)]
pub struct _GstAggregatorPrivate(c_void);

pub type GstAggregatorPrivate = *mut _GstAggregatorPrivate;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GstAggregator {
    pub parent: gst::GstElement,
    pub srcpad: *mut gst::GstPad,
    pub priv_: *mut GstAggregatorPrivate,
    pub _gst_reserved: [gpointer; 20],
}

impl ::std::fmt::Debug for GstAggregator {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct(&format!("GstAggregator @ {:?}", self as *const _))
            .field("parent", &self.parent)
            .field("srcpad", &self.srcpad)
            .finish()
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GstAggregatorPad {
    pub parent: gst::GstPad,
    pub segment: gst::GstSegment,
    pub priv_: *mut GstAggregatorPadPrivate,
    pub _gst_reserved: [gpointer; 4],
}

impl ::std::fmt::Debug for GstAggregatorPad {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        f.debug_struct(&format!("GstAggregatorPad @ {:?}", self as *const _))
            .field("parent", &self.parent)
            .field("segment", &self.segment)
            .finish()
    }
}

extern "C" {
    //=========================================================================
    // GstAggregator
    //=========================================================================
    pub fn gst_aggregator_get_type() -> GType;
    pub fn gst_aggregator_finish_buffer(
        aggregator: *mut GstAggregator,
        buffer: *mut gst::GstBuffer,
    ) -> gst::GstFlowReturn;
    pub fn gst_aggregator_negotiate(aggregator: *mut GstAggregator) -> gboolean;
    pub fn gst_aggregator_get_allocator(
        self_: *mut GstAggregator,
        allocator: *mut *mut gst::GstAllocator,
        params: *mut gst::GstAllocationParams,
    );
    pub fn gst_aggregator_get_buffer_pool(self_: *mut GstAggregator) -> *mut gst::GstBufferPool;
    pub fn gst_aggregator_get_latency(self_: *mut GstAggregator) -> gst::GstClockTime;
    pub fn gst_aggregator_set_latency(
        self_: *mut GstAggregator,
        min_latency: gst::GstClockTime,
        max_latency: gst::GstClockTime,
    );
    pub fn gst_aggregator_set_src_caps(self_: *mut GstAggregator, caps: *mut gst::GstCaps);
    pub fn gst_aggregator_simple_get_next_time(self_: *mut GstAggregator) -> gst::GstClockTime;
    pub fn gst_aggregator_update_segment(self_: *mut GstAggregator, segment: *const gst::GstSegment);

    //=========================================================================
    // GstAggregatorPad
    //=========================================================================
    pub fn gst_aggregator_pad_get_type() -> GType;
    pub fn gst_aggregator_pad_drop_buffer(pad: *mut GstAggregatorPad) -> gboolean;
    pub fn gst_aggregator_pad_has_buffer(pad: *mut GstAggregatorPad) -> gboolean;
    pub fn gst_aggregator_pad_is_eos(pad: *mut GstAggregatorPad) -> gboolean;
    pub fn gst_aggregator_pad_peek_buffer(pad: *mut GstAggregatorPad) -> *mut gst::GstBuffer;
    pub fn gst_aggregator_pad_pop_buffer(pad: *mut GstAggregatorPad) -> *mut gst::GstBuffer;
}
