use glib::prelude::*;

mod imp;

glib::wrapper! {
    pub struct DeviceProvider(ObjectSubclass<imp::DeviceProvider>) @extends gst::DeviceProvider, gst::Object;
}

unsafe impl Send for DeviceProvider {}
unsafe impl Sync for DeviceProvider {}

glib::wrapper! {
    pub struct Device(ObjectSubclass<imp::Device>) @extends gst::Device, gst::Object;
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::DeviceProvider::register(
        Some(plugin),
        "ndideviceprovider",
        gst::Rank::Primary,
        DeviceProvider::static_type(),
    )
}
