use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::{gst_debug, gst_error, gst_log};

use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::ndisrcmeta;

static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "ndisrcdemux",
        gst::DebugColorFlags::empty(),
        Some("NewTek NDI Source Demuxer"),
    )
});

#[derive(Default)]
struct State {
    combiner: gst_base::UniqueFlowCombiner,
    video_pad: Option<gst::Pad>,
    video_caps: Option<gst::Caps>,

    audio_pad: Option<gst::Pad>,
    audio_caps: Option<gst::Caps>,
}

pub struct NdiSrcDemux {
    sinkpad: gst::Pad,
    state: Mutex<State>,
}

#[glib::object_subclass]
impl ObjectSubclass for NdiSrcDemux {
    const NAME: &'static str = "NdiSrcDemux";
    type Type = super::NdiSrcDemux;
    type ParentType = gst::Element;

    fn with_class(klass: &Self::Class) -> Self {
        let templ = klass.pad_template("sink").unwrap();
        let sinkpad = gst::Pad::builder_with_template(&templ, Some("sink"))
            .flags(gst::PadFlags::FIXED_CAPS)
            .chain_function(|pad, parent, buffer| {
                NdiSrcDemux::catch_panic_pad_function(
                    parent,
                    || Err(gst::FlowError::Error),
                    |self_, element| self_.sink_chain(pad, element, buffer),
                )
            })
            .event_function(|pad, parent, event| {
                NdiSrcDemux::catch_panic_pad_function(
                    parent,
                    || false,
                    |self_, element| self_.sink_event(pad, element, event),
                )
            })
            .build();

        Self {
            sinkpad,
            state: Mutex::new(State::default()),
        }
    }
}

impl ObjectImpl for NdiSrcDemux {
    fn constructed(&self, obj: &Self::Type) {
        self.parent_constructed(obj);

        obj.add_pad(&self.sinkpad).unwrap();
    }
}

impl GstObjectImpl for NdiSrcDemux {}

impl ElementImpl for NdiSrcDemux {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "NewTek NDI Source Demuxer",
                "Demuxer/Audio/Video",
                "NewTek NDI source demuxer",
                "Sebastian Dröge <sebastian@centricular.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
            let sink_pad_template = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &gst::Caps::builder("application/x-ndi").build(),
            )
            .unwrap();

            let audio_src_pad_template = gst::PadTemplate::new(
                "audio",
                gst::PadDirection::Src,
                gst::PadPresence::Sometimes,
                &gst::Caps::builder("audio/x-raw").build(),
            )
            .unwrap();

            let video_src_pad_template = gst::PadTemplate::new(
                "video",
                gst::PadDirection::Src,
                gst::PadPresence::Sometimes,
                &gst::Caps::builder("video/x-raw").build(),
            )
            .unwrap();

            vec![
                sink_pad_template,
                audio_src_pad_template,
                video_src_pad_template,
            ]
        });

        PAD_TEMPLATES.as_ref()
    }

    fn change_state(
        &self,
        element: &Self::Type,
        transition: gst::StateChange,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        let res = self.parent_change_state(element, transition)?;

        match transition {
            gst::StateChange::PausedToReady => {
                let mut state = self.state.lock().unwrap();
                for pad in [state.audio_pad.take(), state.video_pad.take()]
                    .iter()
                    .flatten()
                {
                    element.remove_pad(pad).unwrap();
                }
                *state = State::default();
            }
            _ => (),
        }

        Ok(res)
    }
}

impl NdiSrcDemux {
    fn sink_chain(
        &self,
        pad: &gst::Pad,
        element: &super::NdiSrcDemux,
        mut buffer: gst::Buffer,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        gst_log!(CAT, obj: pad, "Handling buffer {:?}", buffer);

        let meta = buffer.make_mut().meta_mut::<ndisrcmeta::NdiSrcMeta>().ok_or_else(|| {
            gst_error!(CAT, obj: element, "Buffer without NDI source meta");
            gst::FlowError::Error
        })?;

        let mut events = vec![];
        let srcpad;
        let mut add_pad = false;

        let mut state = self.state.lock().unwrap();
        let caps = meta.caps();
        match meta.stream_type() {
            ndisrcmeta::StreamType::Audio => {
                if let Some(ref pad) = state.audio_pad {
                    srcpad = pad.clone();
                } else {
                    gst_debug!(CAT, obj: element, "Adding audio pad with caps {}", caps);

                    let klass = element.element_class();
                    let templ = klass.pad_template("audio").unwrap();
                    let pad = gst::Pad::builder_with_template(&templ, Some("audio"))
                        .flags(gst::PadFlags::FIXED_CAPS)
                        .build();

                    let mut caps_event = Some(gst::event::Caps::new(&caps));

                    self.sinkpad.sticky_events_foreach(|ev| {
                        if ev.type_() < gst::EventType::Caps {
                            events.push(ev.clone());
                        } else {
                            if let Some(ev) = caps_event.take() {
                                events.push(ev);
                            }

                            if ev.type_() != gst::EventType::Caps {
                                events.push(ev.clone());
                            }
                        }

                        std::ops::ControlFlow::Continue(gst::EventForeachAction::Keep)
                    });

                    state.audio_caps = Some(caps.clone());
                    state.audio_pad = Some(pad.clone());

                    let _ = pad.set_active(true);
                    for ev in events.drain(..) {
                        let _ = pad.store_sticky_event(&ev);
                    }

                    state.combiner.add_pad(&pad);

                    add_pad = true;
                    srcpad = pad;
                }

                if state.audio_caps.as_ref() != Some(&caps) {
                    gst_debug!(CAT, obj: element, "Audio caps changed to {}", caps);
                    events.push(gst::event::Caps::new(&caps));
                    state.audio_caps = Some(caps);
                }
            }
            ndisrcmeta::StreamType::Video => {
                if let Some(ref pad) = state.video_pad {
                    srcpad = pad.clone();
                } else {
                    gst_debug!(CAT, obj: element, "Adding video pad with caps {}", caps);

                    let klass = element.element_class();
                    let templ = klass.pad_template("video").unwrap();
                    let pad = gst::Pad::builder_with_template(&templ, Some("video"))
                        .flags(gst::PadFlags::FIXED_CAPS)
                        .build();

                    let mut caps_event = Some(gst::event::Caps::new(&caps));

                    self.sinkpad.sticky_events_foreach(|ev| {
                        if ev.type_() < gst::EventType::Caps {
                            events.push(ev.clone());
                        } else {
                            if let Some(ev) = caps_event.take() {
                                events.push(ev);
                            }

                            if ev.type_() != gst::EventType::Caps {
                                events.push(ev.clone());
                            }
                        }

                        std::ops::ControlFlow::Continue(gst::EventForeachAction::Keep)
                    });

                    state.video_caps = Some(caps.clone());
                    state.video_pad = Some(pad.clone());

                    let _ = pad.set_active(true);
                    for ev in events.drain(..) {
                        let _ = pad.store_sticky_event(&ev);
                    }

                    state.combiner.add_pad(&pad);

                    add_pad = true;
                    srcpad = pad;
                }

                if state.video_caps.as_ref() != Some(&caps) {
                    gst_debug!(CAT, obj: element, "Video caps changed to {}", caps);
                    events.push(gst::event::Caps::new(&caps));
                    state.video_caps = Some(caps);
                }
            }
        }
        drop(state);
        meta.remove().unwrap();

        if add_pad {
            element.add_pad(&srcpad).unwrap();
            if element.num_src_pads() == 2 {
                element.no_more_pads();
            }

        }

        for ev in events {
            srcpad.push_event(ev);
        }

        let res = srcpad.push(buffer);

        let mut state = self.state.lock().unwrap();
        state.combiner.update_pad_flow(&srcpad, res)
    }

    fn sink_event(&self,
        pad: &gst::Pad,
        element: &super::NdiSrcDemux,
        event: gst::Event
    ) -> bool {
        use gst::EventView;

        gst_log!(CAT, obj: pad, "Handling event {:?}", event);
        if let EventView::Eos(_) = event.view() {
            if element.num_src_pads() == 0 {
                // error out on EOS if no src pad are available
                gst::element_error!(
                    element,
                    gst::StreamError::Demux,
                    ["EOS without available srcpad(s)"]
                );
            }
        }
        pad.event_default(Some(element), event)
    }

}
