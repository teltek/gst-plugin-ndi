use glib::prelude::*;

mod imp;

glib::wrapper! {
    pub struct NdiSrcDemux(ObjectSubclass<imp::NdiSrcDemux>) @extends gst::Element, gst::Object;
}

unsafe impl Send for NdiSrcDemux {}
unsafe impl Sync for NdiSrcDemux {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndisrcdemux",
        gst::Rank::Primary,
        NdiSrcDemux::static_type(),
    )
}
