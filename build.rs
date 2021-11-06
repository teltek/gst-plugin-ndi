fn main() {
    gst_plugin_version_helper::info();

    if !cfg!(feature = "sink-v1_14") {
        return;
    }

    let gstreamer = pkg_config::probe_library("gstreamer-1.0").unwrap();
    let includes = [gstreamer.include_paths];

    let files = ["src/base/gstaggregator.c"];

    let mut build = cc::Build::new();
    build.include("src/base");

    for f in files.iter() {
        build.file(f);
    }

    for p in includes.iter().flatten() {
        build.include(p);
    }

    build.define(
        "PACKAGE_BUGREPORT",
        "\"https://gitlab.freedesktop.org/gstreamer/gstreamer/issues/new\"",
    );
    build.extra_warnings(false);
    build.define("GstAggregator", "GstAggregatorNdi");
    build.define("GstAggregatorClass", "GstAggregatorNdiClass");
    build.define("GstAggregatorPrivate", "GstAggregatorNdiPrivate");
    build.define("GstAggregatorPad", "GstAggregatorNdiPad");
    build.define("GstAggregatorPadClass", "GstAggregatorNdiPadClass");
    build.define("GstAggregatorPadPrivate", "GstAggregatorNdiPadPrivate");
    build.define("GST_BASE_API", "G_GNUC_INTERNAL");

    build.compile("libgstaggregator-c.a");
}
