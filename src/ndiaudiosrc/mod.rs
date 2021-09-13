use glib::prelude::*;

mod imp;

glib::wrapper! {
    pub struct NdiAudioSrc(ObjectSubclass<imp::NdiAudioSrc>) @extends gst_base::BaseSrc, gst::Element, gst::Object;
}

unsafe impl Send for NdiAudioSrc {}
unsafe impl Sync for NdiAudioSrc {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "ndiaudiosrc",
        gst::Rank::None,
        NdiAudioSrc::static_type(),
    )
}
