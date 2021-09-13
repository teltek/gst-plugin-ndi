use glib::prelude::*;

mod imp;

glib::wrapper! {
    pub struct NdiVideoSrc(ObjectSubclass<imp::NdiVideoSrc>) @extends gst_base::BaseSrc, gst::Element, gst::Object;
}

unsafe impl Send for NdiVideoSrc {}
unsafe impl Sync for NdiVideoSrc {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndivideosrc",
        gst::Rank::None,
        NdiVideoSrc::static_type(),
    )
}
