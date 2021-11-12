GStreamer NDI Plugin for Linux
====================

*Compiled and tested with NDI SDK 4.0, 4.1 and 5.0*

This is a plugin for the [GStreamer](https://gstreamer.freedesktop.org/) multimedia framework that allows GStreamer to receive a stream from a [NDI](https://www.newtek.com/ndi/) source. This plugin has been developed by [Teltek](http://teltek.es/) and was funded by the [University of the Arts London](https://www.arts.ac.uk/) and [The University of Manchester](https://www.manchester.ac.uk/).

Currently the plugin has a source element for receiving from NDI sources, a sink element to provide an NDI source and a device provider for discovering NDI sources on the network.

Some examples of how to use these elements from the command line:

```console
# Information about the elements
$ gst-inspect-1.0 ndi
$ gst-inspect-1.0 ndisrc
$ gst-inspect-1.0 ndisink

# Discover all NDI sources on the network
$ gst-device-monitor-1.0 -f Source/Network:application/x-ndi

# Audio/Video source pipeline
$ gst-launch-1.0 ndisrc ndi-name="GC-DEV2 (OBS)" ! ndisrcdemux name=demux   demux.video ! queue ! videoconvert ! autovideosink  demux.audio ! queue ! audioconvert ! autoaudiosink

# Audio/Video sink pipeline
$ gst-launch-1.0 videotestsrc is-live=true ! video/x-raw,format=UYVY ! ndisinkcombiner name=combiner ! ndisink ndi-name="My NDI source"  audiotestsrc is-live=true ! combiner.audio
```

Feel free to contribute to this project. Some ways you can contribute are:
* Testing with more hardware and software and reporting bugs
* Doing pull requests.

Compilation of the NDI element
-------
To compile the NDI element it's necessary to install Rust, the NDI SDK and the following packages for gstreamer:

```console
$ apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
      gstreamer1.0-plugins-base

```
To install the required NDI library there are two options:
1. Download NDI SDK from NDI website and move the library to the correct location.
2. Use a [deb package](https://github.com/Palakis/obs-ndi/releases/download/4.5.2/libndi3_3.5.1-1_amd64.deb) made by the community. Thanks to [NDI plugin for OBS](https://github.com/Palakis/obs-ndi).

To install Rust, you can follow their documentation: https://www.rust-lang.org/en-US/install.html

Once all requirements are met, you can build the plugin by executing the following command from the project root folder:

```
cargo build
export GST_PLUGIN_PATH=`pwd`/target/debug
gst-inspect-1.0 ndi
```

By defult GStreamer 1.18 is required, to use an older version. You can build with `$ cargo build --no-default-features --features whatever_you_want_to_enable_of_the_above_features`
      

If all went ok, you should see info related to the NDI element. To make the plugin available without using `GST_PLUGIN_PATH` it's necessary to copy the plugin to the gstreamer plugins folder.

```console
$ cargo build --release
$ sudo install -o root -g root -m 644 target/release/libgstndi.so /usr/lib/x86_64-linux-gnu/gstreamer-1.0/
$ sudo ldconfig
$ gst-inspect-1.0 ndi
```

More info about GStreamer plugins written in Rust:
----------------------------------
https://gitlab.freedesktop.org/gstreamer/gstreamer-rs
https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs


License
-------
This plugin is licensed under the LGPL - see the [LICENSE](LICENSE) file for details


Acknowledgments
-------
* University of the Arts London and The University of Manchester.
* Sebastian Dröge (@sdroege).
