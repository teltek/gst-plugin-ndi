use glib::prelude::*;

#[cfg(feature = "sink-v1_14")]
use crate::gst_base_compat as gst_base;

mod imp;

glib::wrapper! {
    pub struct NdiSinkCombiner(ObjectSubclass<imp::NdiSinkCombiner>) @extends gst_base::Aggregator, gst::Element, gst::Object;
}

unsafe impl Send for NdiSinkCombiner {}
unsafe impl Sync for NdiSinkCombiner {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndisinkcombiner",
        gst::Rank::None,
        NdiSinkCombiner::static_type(),
    )
}
