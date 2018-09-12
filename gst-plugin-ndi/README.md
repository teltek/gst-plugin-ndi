GStreamer NDI Plugin for Linux
====================

*Compiled and tested with Ubuntu 16.04.5, GStreamer 1.8.3 and NDI SDK 3.0.9 and 3.5.1*

This is a plugin for the [GStreamer](https://gstreamer.freedesktop.org/) multimedia framework that allows to receive a stream from a [NDI](https://www.newtek.com/ndi/) source. This plugin is developed by [Teltek](http://teltek.es/) and funded by the [University of the Arts London](https://www.arts.ac.uk/) and [The University of Manchester](https://www.manchester.ac.uk/).

Currently the plugin only has sources elements, `ndivideosrc` to get video from the stream and `ndiaudiosrc` for audio. Only it's necessary to provide the name or the ip of the stream, and automatically the element get all the information required from the stream, such resolution, framerate, audio channels,...

Some examples of usage of these elements:
```
#Information about the elements
gst-inspect-1.0 ndi
gst-inspect-1.0 ndivideosrc
gst-inspect-1.0 ndiaudiosrc

#Video pipeline
gst-launch-1.0 ndivideosrc stream-name="GC-DEV2 (OBS)" ! autovideosink
#Audio pipeline
gst-launch-1.0 ndiaudiosrc stream-name="GC-DEV2 (OBS)" ! autoaudiosink

#Video and audio pipeline
gst-launch-1.0 ndivideosrc stream-name="GC-DEV2 (OBS)" ! autovideosink ndiaudiosrc stream-name="GC-DEV2 (OBS)" ! autoaudiosink
```

Feel free to contribute to this project testing with more hardware and software, reporting bugs or with pull requests.

Compile NDI element
-------
Before compile the element it's necessary install Rust, NDI SDK and the following packages for gstreamer:

```
apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
      gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
      gstreamer1.0-libav libgstrtspserver-1.0-dev

```
To install the necessary NDI library exists two options:
* Download NDI SDK from NDI website and move the library to the correct location.
* Use a [deb package](https://github.com/Palakis/obs-ndi/releases/download/4.5.2/libndi3_3.5.1-1_amd64.deb) made by the community. Thanks to [NDI plugin for OBS](https://github.com/Palakis/obs-ndi).

To build the plugin execute these commands from the root of the repository folder

```
cargo build

export GST_PLUGIN_PATH=`pwd`/target/debug
gst-inspect-1.0 ndi
```

If all went ok, you should see info related to the NDI element. To make the plugin available without using `GST_PLUGIN_PATH` it's necessary copy the plugin to the gstreamer plugins folder.
```
cp target/debug/libgstndi.so /usr/lib/x86_64-linux-gnu/gstreamer-1.0/
```

More info about GStreamer plugins written in Rust:
----------------------------------
https://github.com/sdroege/gstreamer-rs  
https://github.com/sdroege/gst-plugin-rs

https://coaxion.net/blog/2018/01/how-to-write-gstreamer-elements-in-rust-part-1-a-video-filter-for-converting-rgb-to-grayscale/  
https://coaxion.net/blog/2018/02/how-to-write-gstreamer-elements-in-rust-part-2-a-raw-audio-sine-wave-source/
