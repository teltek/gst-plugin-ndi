// Copyright (C) 2017 Sebastian Dröge <sebastian@centricular.com>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern crate glib;
#[macro_use]
extern crate gst_plugin;
#[macro_use]
extern crate gstreamer as gst;
extern crate gstreamer_audio as gst_audio;
extern crate gstreamer_base as gst_base;
extern crate gstreamer_video as gst_video;

extern crate byte_slice_cast;
extern crate num_traits;

mod ndivideosrc;
mod ndiaudiosrc;
pub mod ndilib;

// Plugin entry point that should register all elements provided by this plugin,
// and everything else that this plugin might provide (e.g. typefinders or device providers).
fn plugin_init(plugin: &gst::Plugin) -> bool {
    ndivideosrc::register(plugin);
    ndiaudiosrc::register(plugin);
    true
}

// Static plugin metdata that is directly stored in the plugin shared object and read by GStreamer
// upon loading.
// Plugin name, plugin description, plugin entry point function, version number of this plugin,
// license of the plugin, source package name, binary package name, origin where it comes from
// and the date/time of release.
plugin_define!(
    b"ndi\0",
    b"NewTek NDI Plugin\0",
    plugin_init,
    b"1.0\0",
    b"MIT/X11\0",
    b"ndi\0",
    b"ndi\0",
    b"https://gitlab.teltek.es/rubenrua/ndi-rs.git\0",
    b"2018-04-09\0"
);
