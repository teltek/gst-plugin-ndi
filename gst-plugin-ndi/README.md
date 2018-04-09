TODO
====

See:

https://coaxion.net/blog/2018/01/how-to-write-gstreamer-elements-in-rust-part-1-a-video-filter-for-converting-rgb-to-grayscale/
https://coaxion.net/blog/2018/02/how-to-write-gstreamer-elements-in-rust-part-2-a-raw-audio-sine-wave-source/


Test
-------

```
cargo build
export GST_PLUGIN_PATH=`pwd`/target/debug
gst-inspect-1.0 ndisrc
```
