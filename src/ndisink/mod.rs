use glib::prelude::*;

mod imp;

glib::wrapper! {
    pub struct NdiSink(ObjectSubclass<imp::NdiSink>) @extends gst_base::BaseSink, gst::Element, gst::Object;
}

unsafe impl Send for NdiSink {}
unsafe impl Sync for NdiSink {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndisink",
        gst::Rank::None,
        NdiSink::static_type(),
    )
}
