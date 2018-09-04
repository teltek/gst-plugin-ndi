GStreamer NDI Plugin
====================

*Compiled and tested with Ubuntu 16.04.5 and GStreamer 1.8.3*

Before compile the element it's necessary install Rust, NDI SDK and the following packages for gstreamer:

```
apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
      gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
      gstreamer1.0-libav libgstrtspserver-1.0-dev
```


Compile NDI element and basic pipelines
-------

```
cargo build
export GST_PLUGIN_PATH=`pwd`/target/debug

gst-inspect-1.0 ndi
gst-inspect-1.0 ndivideosrc
gst-inspect-1.0 ndiaudiosrc

gst-launch-1.0 ndivideosrc stream-name="GC-DEV2 (OBS)" ! autovideosink
gst-launch-1.0 ndiaudiosrc stream-name="GC-DEV2 (OBS)" ! autoaudiosink

gst-launch-1.0 ndivideosrc stream-name="GC-DEV2 (OBS)" ! autovideosink ndiaudiosrc stream-name="GC-DEV2 (OBS)" ! autoaudiosink

```

Debug pipelines:
```
#Check if the timestamps are correct
gst-launch-1.0 -v ndivideosrc name=gc-ndi-src stream-name="GC-DEV2 (OBS)" ! fakesink silent=false

#Debug sink to check if jitter is correct
GST_DEBUG=*basesink*:5 gst-launch-1.0 -v ndivideosrc name=gc-ndi-src stream-name="GC-DEV2 (OBS)" ! autovideosink

#Add latency when launching the pipeline
gst-launch-1.0 -v ndivideosrc name=gc-ndi-src stream-name="GC-DEV2 (OBS)" ! autovideosink ts-offset=1000000000
```

More info about GStreamer plugins and Rust:
----------------------------------
https://coaxion.net/blog/2018/01/how-to-write-gstreamer-elements-in-rust-part-1-a-video-filter-for-converting-rgb-to-grayscale/  
https://coaxion.net/blog/2018/02/how-to-write-gstreamer-elements-in-rust-part-2-a-raw-audio-sine-wave-source/
