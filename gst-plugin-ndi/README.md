TODO
====

See:

https://coaxion.net/blog/2018/01/how-to-write-gstreamer-elements-in-rust-part-1-a-video-filter-for-converting-rgb-to-grayscale/
https://coaxion.net/blog/2018/02/how-to-write-gstreamer-elements-in-rust-part-2-a-raw-audio-sine-wave-source/

Before cargo build install:

```
$ apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
      gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
      gstreamer1.0-libav libgstrtspserver-1.0-dev
```



Test
-------

```
cargo build
export GST_PLUGIN_PATH=`pwd`/target/debug
gst-inspect-1.0 ndisrc
GST_DEBUG=3 gst-launch-1.0 ndisrc ! video/x-raw, format=UYVY, width=720, height=576, framerate=1/25 ! videoconvert ! autovideosink

GST_DEBUG=3 gst-launch-1.0 -v ndisrc stream-name="GC-DEV2 (Nombre_del_stream)" ! video/x-raw, format=UYVY, width=720, height=576, framerate=1/25 ! xvimagesink sync=false
```
