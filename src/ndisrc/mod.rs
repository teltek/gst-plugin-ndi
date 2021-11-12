use glib::prelude::*;

mod imp;

glib::wrapper! {
    pub struct NdiSrc(ObjectSubclass<imp::NdiSrc>) @extends gst_base::BaseSrc, gst::Element, gst::Object;
}

unsafe impl Send for NdiSrc {}
unsafe impl Sync for NdiSrc {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndisrc",
        gst::Rank::None,
        NdiSrc::static_type(),
    )
}
