use glib::prelude::*;
use gst::prelude::*;
use gst::{gst_debug, gst_error, gst_log, gst_warning};
use gst_video::prelude::*;

use byte_slice_cast::AsMutSliceOf;

use std::cmp;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;

use super::*;

static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "ndireceiver",
        gst::DebugColorFlags::empty(),
        Some("NewTek NDI receiver"),
    )
});

#[derive(Clone)]
pub struct Receiver(Arc<ReceiverInner>);

#[derive(Debug)]
pub enum Buffer {
    Audio(gst::Buffer, gst_audio::AudioInfo),
    Video(gst::Buffer, gst_video::VideoInfo),
}

#[derive(Debug)]
pub enum ReceiverItem {
    Buffer(Buffer),
    Flushing,
    Timeout,
    Error(gst::FlowError),
}

pub struct ReceiverInner {
    queue: ReceiverQueue,
    max_queue_length: usize,

    observations: Observations,

    element: glib::WeakRef<gst_base::BaseSrc>,
    timestamp_mode: TimestampMode,

    timeout: u32,
    connect_timeout: u32,

    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[derive(Clone)]
struct ReceiverQueue(Arc<(Mutex<ReceiverQueueInner>, Condvar)>);

struct ReceiverQueueInner {
    // Set to true when the capture thread should be stopped
    shutdown: bool,

    // If we're flushing right now and all buffers should simply be discarded
    // and capture() directly returns Flushing
    flushing: bool,

    // If we're playing right now or not: if not we simply discard everything captured
    playing: bool,
    // Queue containing our buffers. This holds at most 5 buffers at a time.
    //
    // On timeout/error will contain a single item and then never be filled again
    buffer_queue: VecDeque<Buffer>,

    error: Option<gst::FlowError>,
    timeout: bool,
}

// 100 frames observations window over which we calculate the timestamp drift
// between sender and receiver. A bigger window allows more smoothing out of
// network effects
const WINDOW_LENGTH: usize = 100;
#[derive(Clone)]
struct Observations(Arc<Mutex<ObservationsInner>>);
struct ObservationsInner {
    // NDI timestamp - GStreamer clock time tuples
    values: Vec<(u64, u64)>,
    values_tmp: [(u64, u64); WINDOW_LENGTH],
    current_mapping: TimeMapping,
    next_mapping: TimeMapping,
    time_mapping_pending: bool,

    // How many frames we skipped since last observation
    // we took
    skip_count: usize,
    // How many frames we skip in this period. once skip_count
    // reaches this, we take another observation
    skip_period: usize,
    // How many observations are left until we update the skip_period
    // again. This is always initialized to WINDOW_LENGTH
    skip_period_update_in: usize,
}

#[derive(Clone, Copy, Debug)]
struct TimeMapping {
    xbase: u64,
    b: u64,
    num: u64,
    den: u64,
}

impl Observations {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(ObservationsInner {
            values: Vec::with_capacity(WINDOW_LENGTH),
            values_tmp: [(0, 0); WINDOW_LENGTH],
            current_mapping: TimeMapping::default(),
            next_mapping: TimeMapping::default(),
            time_mapping_pending: false,
            skip_count: 0,
            skip_period: 1,
            skip_period_update_in: WINDOW_LENGTH,
        })))
    }

    fn process(
        &self,
        element: &gst_base::BaseSrc,
        time: (Option<gst::ClockTime>, gst::ClockTime),
        duration: Option<gst::ClockTime>,
    ) -> (gst::ClockTime, Option<gst::ClockTime>) {
        if time.0.is_none() {
            return (time.1, duration);
        }

        let time = (time.0.unwrap(), time.1);

        let mut inner = self.0.lock().unwrap();
        let ObservationsInner {
            ref mut values,
            ref mut values_tmp,
            ref mut current_mapping,
            ref mut next_mapping,
            ref mut time_mapping_pending,
            ref mut skip_count,
            ref mut skip_period,
            ref mut skip_period_update_in,
        } = *inner;

        if values.is_empty() {
            current_mapping.xbase = time.0.nseconds();
            current_mapping.b = time.1.nseconds();
            current_mapping.num = 1;
            current_mapping.den = 1;
        }

        if *skip_count == 0 {
            *skip_count += 1;
            if *skip_count >= *skip_period {
                *skip_count = 0;
            }
            *skip_period_update_in -= 1;
            if *skip_period_update_in == 0 {
                *skip_period_update_in = WINDOW_LENGTH;

                // Start by first updating every frame, then every second frame, then every third
                // frame, etc. until we update once every quarter second
                let framerate = gst::ClockTime::SECOND
                    .checked_div(duration.unwrap_or(40 * gst::ClockTime::MSECOND).nseconds())
                    .unwrap_or(25) as usize;

                if *skip_period < framerate / 4 + 1 {
                    *skip_period += 1;
                } else {
                    *skip_period = framerate / 4 + 1;
                }
            }

            assert!(values.len() <= WINDOW_LENGTH);

            if values.len() == WINDOW_LENGTH {
                values.remove(0);
            }
            values.push((time.0.nseconds(), time.1.nseconds()));

            if let Some((num, den, b, xbase, r_squared)) =
                gst::calculate_linear_regression(values, Some(values_tmp))
            {
                next_mapping.xbase = xbase;
                next_mapping.b = b;
                next_mapping.num = num;
                next_mapping.den = den;
                *time_mapping_pending = true;
                gst_debug!(
                    CAT,
                    obj: element,
                    "Calculated new time mapping: GStreamer time = {} * (NDI time - {}) + {} ({})",
                    next_mapping.num as f64 / next_mapping.den as f64,
                    gst::ClockTime::from_nseconds(next_mapping.xbase),
                    gst::ClockTime::from_nseconds(next_mapping.b),
                    r_squared,
                );
            }
        } else {
            *skip_count += 1;
            if *skip_count >= *skip_period {
                *skip_count = 0;
            }
        }

        if *time_mapping_pending {
            let expected = gst::Clock::adjust_with_calibration(
                time.0,
                gst::ClockTime::from_nseconds(current_mapping.xbase),
                gst::ClockTime::from_nseconds(current_mapping.b),
                gst::ClockTime::from_nseconds(current_mapping.num),
                gst::ClockTime::from_nseconds(current_mapping.den),
            );
            let new_calculated = gst::Clock::adjust_with_calibration(
                time.0,
                gst::ClockTime::from_nseconds(next_mapping.xbase),
                gst::ClockTime::from_nseconds(next_mapping.b),
                gst::ClockTime::from_nseconds(next_mapping.num),
                gst::ClockTime::from_nseconds(next_mapping.den),
            );

            let diff = if new_calculated > expected {
                new_calculated - expected
            } else {
                expected - new_calculated
            };

            // Allow at most 5% frame duration or 2ms difference per frame
            let max_diff = cmp::max(
                (duration.map(|d| d / 10)).unwrap_or(2 * gst::ClockTime::MSECOND),
                2 * gst::ClockTime::MSECOND,
            );

            if diff > max_diff {
                gst_debug!(
                    CAT,
                    obj: element,
                    "New time mapping causes difference {} but only {} allowed",
                    diff,
                    max_diff,
                );

                if new_calculated > expected {
                    current_mapping.b = (expected + max_diff).nseconds();
                    current_mapping.xbase = time.0.nseconds();
                } else {
                    current_mapping.b = (expected - max_diff).nseconds();
                    current_mapping.xbase = time.0.nseconds();
                }
            } else {
                *current_mapping = *next_mapping;
            }
        }

        let converted_timestamp = gst::Clock::adjust_with_calibration(
            time.0,
            gst::ClockTime::from_nseconds(current_mapping.xbase),
            gst::ClockTime::from_nseconds(current_mapping.b),
            gst::ClockTime::from_nseconds(current_mapping.num),
            gst::ClockTime::from_nseconds(current_mapping.den),
        );
        let converted_duration =
            duration.and_then(|d| d.mul_div_floor(current_mapping.num, current_mapping.den));

        gst_debug!(
            CAT,
            obj: element,
            "Converted timestamp {}/{} to {}, duration {} to {}",
            time.0,
            time.1,
            converted_timestamp.display(),
            duration.display(),
            converted_duration.display(),
        );

        (converted_timestamp, converted_duration)
    }
}

impl Default for TimeMapping {
    fn default() -> Self {
        Self {
            xbase: 0,
            b: 0,
            num: 1,
            den: 1,
        }
    }
}

#[derive(Clone)]
pub struct ReceiverControlHandle {
    queue: ReceiverQueue,
}

impl ReceiverControlHandle {
    pub fn set_flushing(&self, flushing: bool) {
        let mut queue = (self.queue.0).0.lock().unwrap();
        queue.flushing = flushing;
        (self.queue.0).1.notify_all();
    }

    pub fn set_playing(&self, playing: bool) {
        let mut queue = (self.queue.0).0.lock().unwrap();
        queue.playing = playing;
    }

    pub fn shutdown(&self) {
        let mut queue = (self.queue.0).0.lock().unwrap();
        queue.shutdown = true;
        (self.queue.0).1.notify_all();
    }
}

impl Drop for ReceiverInner {
    fn drop(&mut self) {
        // Will shut down the receiver thread on the next iteration
        let mut queue = (self.queue.0).0.lock().unwrap();
        queue.shutdown = true;
        drop(queue);

        let element = self.element.upgrade();

        if let Some(ref element) = element {
            gst_debug!(CAT, obj: element, "Closed NDI connection");
        }
    }
}

impl Receiver {
    fn new(
        recv: RecvInstance,
        timestamp_mode: TimestampMode,
        timeout: u32,
        connect_timeout: u32,
        max_queue_length: usize,
        element: &gst_base::BaseSrc,
    ) -> Self {
        let receiver = Receiver(Arc::new(ReceiverInner {
            queue: ReceiverQueue(Arc::new((
                Mutex::new(ReceiverQueueInner {
                    shutdown: false,
                    playing: false,
                    flushing: false,
                    buffer_queue: VecDeque::with_capacity(max_queue_length),
                    error: None,
                    timeout: false,
                }),
                Condvar::new(),
            ))),
            max_queue_length,
            observations: Observations::new(),
            element: element.downgrade(),
            timestamp_mode,
            timeout,
            connect_timeout,
            thread: Mutex::new(None),
        }));

        let weak = Arc::downgrade(&receiver.0);
        let thread = thread::spawn(move || {
            use std::panic;

            let weak_clone = weak.clone();
            match panic::catch_unwind(panic::AssertUnwindSafe(move || {
                Self::receive_thread(&weak_clone, recv)
            })) {
                Ok(_) => (),
                Err(_) => {
                    if let Some(receiver) = weak.upgrade().map(Receiver) {
                        if let Some(element) = receiver.0.element.upgrade() {
                            gst::element_error!(
                                element,
                                gst::LibraryError::Failed,
                                ["Panic while connecting to NDI source"]
                            );
                        }

                        let mut queue = (receiver.0.queue.0).0.lock().unwrap();
                        queue.error = Some(gst::FlowError::Error);
                        (receiver.0.queue.0).1.notify_one();
                    }
                }
            }
        });

        *receiver.0.thread.lock().unwrap() = Some(thread);

        receiver
    }

    pub fn receiver_control_handle(&self) -> ReceiverControlHandle {
        ReceiverControlHandle {
            queue: self.0.queue.clone(),
        }
    }

    pub fn set_flushing(&self, flushing: bool) {
        let mut queue = (self.0.queue.0).0.lock().unwrap();
        queue.flushing = flushing;
        (self.0.queue.0).1.notify_all();
    }

    pub fn set_playing(&self, playing: bool) {
        let mut queue = (self.0.queue.0).0.lock().unwrap();
        queue.playing = playing;
    }

    pub fn shutdown(&self) {
        let mut queue = (self.0.queue.0).0.lock().unwrap();
        queue.shutdown = true;
        (self.0.queue.0).1.notify_all();
    }

    pub fn capture(&self) -> ReceiverItem {
        let mut queue = (self.0.queue.0).0.lock().unwrap();
        loop {
            if let Some(err) = queue.error {
                return ReceiverItem::Error(err);
            } else if queue.buffer_queue.is_empty() && queue.timeout {
                return ReceiverItem::Timeout;
            } else if queue.flushing || queue.shutdown {
                return ReceiverItem::Flushing;
            } else if let Some(buffer) = queue.buffer_queue.pop_front() {
                return ReceiverItem::Buffer(buffer);
            }

            queue = (self.0.queue.0).1.wait(queue).unwrap();
        }
    }

    pub fn connect(
        element: &gst_base::BaseSrc,
        ndi_name: Option<&str>,
        url_address: Option<&str>,
        receiver_ndi_name: &str,
        connect_timeout: u32,
        bandwidth: NDIlib_recv_bandwidth_e,
        timestamp_mode: TimestampMode,
        timeout: u32,
        max_queue_length: usize,
    ) -> Option<Self> {
        gst_debug!(CAT, obj: element, "Starting NDI connection...");

        assert!(ndi_name.is_some() || url_address.is_some());

        gst_debug!(
            CAT,
            obj: element,
            "Connecting to NDI source with NDI name '{:?}' and URL/Address {:?}",
            ndi_name,
            url_address,
        );

        // FIXME: Ideally we would use NDIlib_recv_color_format_fastest here but that seems to be
        // broken with interlaced content currently
        let recv = RecvInstance::builder(ndi_name, url_address, receiver_ndi_name)
            .bandwidth(bandwidth)
            .color_format(NDIlib_recv_color_format_e::NDIlib_recv_color_format_UYVY_BGRA)
            .allow_video_fields(true)
            .build();
        let recv = match recv {
            None => {
                gst::element_error!(
                    element,
                    gst::CoreError::Negotiation,
                    ["Failed to connect to source"]
                );
                return None;
            }
            Some(recv) => recv,
        };

        recv.set_tally(&Tally::default());

        let enable_hw_accel = MetadataFrame::new(0, Some("<ndi_hwaccel enabled=\"true\"/>"));
        recv.send_metadata(&enable_hw_accel);

        // This will set info.audio/video accordingly
        let receiver = Receiver::new(
            recv,
            timestamp_mode,
            timeout,
            connect_timeout,
            max_queue_length,
            element,
        );

        Some(receiver)
    }

    fn receive_thread(receiver: &Weak<ReceiverInner>, recv: RecvInstance) {
        let mut first_frame = true;
        let mut timer = time::Instant::now();

        // Capture until error or shutdown
        loop {
            let receiver = match receiver.upgrade().map(Receiver) {
                None => break,
                Some(receiver) => receiver,
            };

            let element = match receiver.0.element.upgrade() {
                None => return,
                Some(element) => element,
            };

            let flushing = {
                let queue = (receiver.0.queue.0).0.lock().unwrap();
                if queue.shutdown {
                    gst_debug!(CAT, obj: &element, "Shutting down");
                    break;
                }

                // If an error happened in the meantime, just go out of here
                if queue.error.is_some() {
                    gst_error!(CAT, obj: &element, "Error while waiting for connection");
                    return;
                }

                queue.flushing
            };

            let timeout = if first_frame {
                receiver.0.connect_timeout
            } else {
                receiver.0.timeout
            };

            let res = match recv.capture(50) {
                _ if flushing => {
                    gst_debug!(CAT, obj: &element, "Flushing");
                    Err(gst::FlowError::Flushing)
                }
                Err(_) => {
                    gst::element_error!(
                        element,
                        gst::ResourceError::Read,
                        ["Error receiving frame"]
                    );
                    Err(gst::FlowError::Error)
                }
                Ok(None) if timeout > 0 && timer.elapsed().as_millis() >= timeout as u128 => {
                    gst_debug!(CAT, obj: &element, "Timed out -- assuming EOS",);
                    Err(gst::FlowError::Eos)
                }
                Ok(None) => {
                    gst_debug!(CAT, obj: &element, "No frame received yet, retry");
                    continue;
                }
                Ok(Some(Frame::Video(frame))) => {
                    first_frame = false;
                    receiver.create_video_buffer_and_info(&element, frame)
                }
                Ok(Some(Frame::Audio(frame))) => {
                    first_frame = false;
                    receiver.create_audio_buffer_and_info(&element, frame)
                }
                Ok(Some(Frame::Metadata(frame))) => {
                    if let Some(metadata) = frame.metadata() {
                        gst_debug!(
                            CAT,
                            obj: &element,
                            "Received metadata at timecode {}: {}",
                            gst::ClockTime::from_nseconds(frame.timecode() as u64 * 100),
                            metadata,
                        );
                    }

                    continue;
                }
            };

            match res {
                Ok(item) => {
                    let mut queue = (receiver.0.queue.0).0.lock().unwrap();
                    while queue.buffer_queue.len() > receiver.0.max_queue_length {
                        gst_warning!(
                            CAT,
                            obj: &element,
                            "Dropping old buffer -- queue has {} items",
                            queue.buffer_queue.len()
                        );
                        queue.buffer_queue.pop_front();
                    }
                    queue.buffer_queue.push_back(item);
                    (receiver.0.queue.0).1.notify_one();
                    timer = time::Instant::now();
                }
                Err(gst::FlowError::Eos) => {
                    gst_debug!(CAT, obj: &element, "Signalling EOS");
                    let mut queue = (receiver.0.queue.0).0.lock().unwrap();
                    queue.timeout = true;
                    (receiver.0.queue.0).1.notify_one();
                    break;
                }
                Err(gst::FlowError::Flushing) => {
                    // Flushing, nothing to be done here except for emptying our queue
                    let mut queue = (receiver.0.queue.0).0.lock().unwrap();
                    queue.buffer_queue.clear();
                    (receiver.0.queue.0).1.notify_one();
                    timer = time::Instant::now();
                }
                Err(err) => {
                    gst_error!(CAT, obj: &element, "Signalling error");
                    let mut queue = (receiver.0.queue.0).0.lock().unwrap();
                    if queue.error.is_none() {
                        queue.error = Some(err);
                    }
                    (receiver.0.queue.0).1.notify_one();
                    break;
                }
            }
        }
    }

    fn calculate_timestamp(
        &self,
        element: &gst_base::BaseSrc,
        timestamp: i64,
        timecode: i64,
        duration: Option<gst::ClockTime>,
    ) -> Option<(gst::ClockTime, Option<gst::ClockTime>)> {
        let clock = element.clock()?;

        // For now take the current running time as PTS. At a later time we
        // will want to work with the timestamp given by the NDI SDK if available
        let now = clock.time()?;
        let base_time = element.base_time()?;
        let receive_time = now - base_time;

        let real_time_now = gst::ClockTime::from_nseconds(glib::real_time() as u64 * 1000);
        let timestamp = if timestamp == ndisys::NDIlib_recv_timestamp_undefined {
            gst::ClockTime::NONE
        } else {
            Some(gst::ClockTime::from_nseconds(timestamp as u64 * 100))
        };
        let timecode = gst::ClockTime::from_nseconds(timecode as u64 * 100);

        gst_log!(
            CAT,
            obj: element,
            "Received frame with timecode {}, timestamp {}, duration {}, receive time {}, local time now {}",
            timecode,
            timestamp.display(),
            duration.display(),
            receive_time.display(),
            real_time_now,
        );

        let (pts, duration) = match self.0.timestamp_mode {
            TimestampMode::ReceiveTimeTimecode => {
                self.0
                    .observations
                    .process(element, (Some(timecode), receive_time), duration)
            }
            TimestampMode::ReceiveTimeTimestamp => {
                self.0
                    .observations
                    .process(element, (timestamp, receive_time), duration)
            }
            TimestampMode::Timecode => (timecode, duration),
            TimestampMode::Timestamp if timestamp.is_none() => (receive_time, duration),
            TimestampMode::Timestamp => {
                // Timestamps are relative to the UNIX epoch
                let timestamp = timestamp?;
                if real_time_now > timestamp {
                    let diff = real_time_now - timestamp;
                    if diff > receive_time {
                        (gst::ClockTime::ZERO, duration)
                    } else {
                        (receive_time - diff, duration)
                    }
                } else {
                    let diff = timestamp - real_time_now;
                    (receive_time + diff, duration)
                }
            }
            TimestampMode::ReceiveTime => (receive_time, duration),
        };

        gst_log!(
            CAT,
            obj: element,
            "Calculated PTS {}, duration {}",
            pts.display(),
            duration.display(),
        );

        Some((pts, duration))
    }

    fn create_video_buffer_and_info(
        &self,
        element: &gst_base::BaseSrc,
        video_frame: VideoFrame,
    ) -> Result<Buffer, gst::FlowError> {
        gst_debug!(CAT, obj: element, "Received video frame {:?}", video_frame);

        let (pts, duration) = self
            .calculate_video_timestamp(element, &video_frame)
            .ok_or_else(|| {
                gst_debug!(CAT, obj: element, "Flushing, dropping buffer");
                gst::FlowError::Flushing
            })?;

        let info = self.create_video_info(element, &video_frame)?;

        let buffer = self.create_video_buffer(element, pts, duration, &info, &video_frame);

        gst_log!(CAT, obj: element, "Produced video buffer {:?}", buffer);

        Ok(Buffer::Video(buffer, info))
    }

    fn calculate_video_timestamp(
        &self,
        element: &gst_base::BaseSrc,
        video_frame: &VideoFrame,
    ) -> Option<(gst::ClockTime, Option<gst::ClockTime>)> {
        let duration = gst::ClockTime::SECOND.mul_div_floor(
            video_frame.frame_rate().1 as u64,
            video_frame.frame_rate().0 as u64,
        );

        self.calculate_timestamp(
            element,
            video_frame.timestamp(),
            video_frame.timecode(),
            duration,
        )
    }

    fn create_video_info(
        &self,
        element: &gst_base::BaseSrc,
        video_frame: &VideoFrame,
    ) -> Result<gst_video::VideoInfo, gst::FlowError> {
        // YV12 and I420 are swapped in the NDI SDK compared to GStreamer
        let format = match video_frame.fourcc() {
            ndisys::NDIlib_FourCC_video_type_UYVY => gst_video::VideoFormat::Uyvy,
            // FIXME: This drops the alpha plane!
            ndisys::NDIlib_FourCC_video_type_UYVA => gst_video::VideoFormat::Uyvy,
            ndisys::NDIlib_FourCC_video_type_YV12 => gst_video::VideoFormat::I420,
            ndisys::NDIlib_FourCC_video_type_NV12 => gst_video::VideoFormat::Nv12,
            ndisys::NDIlib_FourCC_video_type_I420 => gst_video::VideoFormat::Yv12,
            ndisys::NDIlib_FourCC_video_type_BGRA => gst_video::VideoFormat::Bgra,
            ndisys::NDIlib_FourCC_video_type_BGRX => gst_video::VideoFormat::Bgrx,
            ndisys::NDIlib_FourCC_video_type_RGBA => gst_video::VideoFormat::Rgba,
            ndisys::NDIlib_FourCC_video_type_RGBX => gst_video::VideoFormat::Rgbx,
            _ => {
                gst::element_error!(
                    element,
                    gst::StreamError::Format,
                    ["Unsupported video fourcc {:08x}", video_frame.fourcc()]
                );

                return Err(gst::FlowError::NotNegotiated);
            } // TODO: NDIlib_FourCC_video_type_P216 and NDIlib_FourCC_video_type_PA16 not
              // supported by GStreamer
        };

        let par = gst::Fraction::approximate_f32(video_frame.picture_aspect_ratio())
            .unwrap_or_else(|| gst::Fraction::new(1, 1))
            * gst::Fraction::new(video_frame.yres(), video_frame.xres());

        #[cfg(feature = "interlaced-fields")]
        {
            let mut builder = gst_video::VideoInfo::builder(
                format,
                video_frame.xres() as u32,
                video_frame.yres() as u32,
            )
            .fps(gst::Fraction::from(video_frame.frame_rate()))
            .par(par)
            .interlace_mode(match video_frame.frame_format_type() {
                ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_progressive => {
                    gst_video::VideoInterlaceMode::Progressive
                }
                ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_interleaved => {
                    gst_video::VideoInterlaceMode::Interleaved
                }
                _ => gst_video::VideoInterlaceMode::Alternate,
            });

            if video_frame.frame_format_type()
                == ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_interleaved
            {
                builder = builder.field_order(gst_video::VideoFieldOrder::TopFieldFirst);
            }

            builder.build().map_err(|_| {
                gst::element_error!(
                    element,
                    gst::StreamError::Format,
                    ["Invalid video format configuration"]
                );

                gst::FlowError::NotNegotiated
            })
        }

        #[cfg(not(feature = "interlaced-fields"))]
        {
            if video_frame.frame_format_type()
                != ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_progressive
                && video_frame.frame_format_type()
                    != ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_interleaved
            {
                gst::element_error!(
                    element,
                    gst::StreamError::Format,
                    ["Separate field interlacing not supported"]
                );
                return Err(gst::FlowError::NotNegotiated);
            }

            let mut builder = gst_video::VideoInfo::builder(
                format,
                video_frame.xres() as u32,
                video_frame.yres() as u32,
            )
            .fps(gst::Fraction::from(video_frame.frame_rate()))
            .par(par)
            .interlace_mode(
                if video_frame.frame_format_type()
                    == ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_progressive
                {
                    gst_video::VideoInterlaceMode::Progressive
                } else {
                    gst_video::VideoInterlaceMode::Interleaved
                },
            );

            if video_frame.frame_format_type()
                == ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_interleaved
            {
                builder = builder.field_order(gst_video::VideoFieldOrder::TopFieldFirst);
            }

            builder.build().map_err(|_| {
                gst::element_error!(
                    element,
                    gst::StreamError::Format,
                    ["Invalid video format configuration"]
                );

                gst::FlowError::NotNegotiated
            })
        }
    }

    fn create_video_buffer(
        &self,
        element: &gst_base::BaseSrc,
        pts: gst::ClockTime,
        duration: Option<gst::ClockTime>,
        info: &gst_video::VideoInfo,
        video_frame: &VideoFrame,
    ) -> gst::Buffer {
        let mut buffer = gst::Buffer::with_size(info.size()).unwrap();
        {
            let buffer = buffer.get_mut().unwrap();
            buffer.set_pts(pts);
            buffer.set_duration(duration);

            #[cfg(feature = "reference-timestamps")]
            {
                gst::ReferenceTimestampMeta::add(
                    buffer,
                    &*TIMECODE_CAPS,
                    gst::ClockTime::from_nseconds(video_frame.timecode() as u64 * 100),
                    gst::ClockTime::NONE,
                );
                if video_frame.timestamp() != ndisys::NDIlib_recv_timestamp_undefined {
                    gst::ReferenceTimestampMeta::add(
                        buffer,
                        &*TIMESTAMP_CAPS,
                        gst::ClockTime::from_nseconds(video_frame.timestamp() as u64 * 100),
                        gst::ClockTime::NONE,
                    );
                }
            }

            #[cfg(feature = "interlaced-fields")]
            {
                match video_frame.frame_format_type() {
                    ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_interleaved => {
                        buffer.set_video_flags(
                            gst_video::VideoBufferFlags::INTERLACED
                                | gst_video::VideoBufferFlags::TFF,
                        );
                    }
                    ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_field_0 => {
                        buffer.set_video_flags(
                            gst_video::VideoBufferFlags::INTERLACED
                                | gst_video::VideoBufferFlags::TOP_FIELD,
                        );
                    }
                    ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_field_1 => {
                        buffer.set_video_flags(
                            gst_video::VideoBufferFlags::INTERLACED
                                | gst_video::VideoBufferFlags::BOTTOM_FIELD,
                        );
                    }
                    _ => (),
                };
            }

            #[cfg(not(feature = "interlaced-fields"))]
            {
                if video_frame.frame_format_type()
                    == ndisys::NDIlib_frame_format_type_e::NDIlib_frame_format_type_interleaved
                {
                    buffer.set_video_flags(
                        gst_video::VideoBufferFlags::INTERLACED | gst_video::VideoBufferFlags::TFF,
                    );
                }
            }
        }

        self.copy_video_frame(element, info, buffer, video_frame)
    }

    fn copy_video_frame(
        &self,
        _element: &gst_base::BaseSrc,
        info: &gst_video::VideoInfo,
        buffer: gst::Buffer,
        video_frame: &VideoFrame,
    ) -> gst::Buffer {
        let mut vframe = gst_video::VideoFrame::from_buffer_writable(buffer, info).unwrap();

        match info.format() {
            gst_video::VideoFormat::Uyvy
            | gst_video::VideoFormat::Bgra
            | gst_video::VideoFormat::Bgrx
            | gst_video::VideoFormat::Rgba
            | gst_video::VideoFormat::Rgbx => {
                let line_bytes = if info.format() == gst_video::VideoFormat::Uyvy {
                    2 * vframe.width() as usize
                } else {
                    4 * vframe.width() as usize
                };
                let dest_stride = vframe.plane_stride()[0] as usize;
                let dest = vframe.plane_data_mut(0).unwrap();
                let src_stride = video_frame.line_stride_or_data_size_in_bytes() as usize;
                let src = video_frame.data();

                for (dest, src) in dest
                    .chunks_exact_mut(dest_stride)
                    .zip(src.chunks_exact(src_stride))
                {
                    dest.copy_from_slice(src);
                    dest.copy_from_slice(&src[..line_bytes]);
                }
            }
            gst_video::VideoFormat::Nv12 => {
                // First plane
                {
                    let line_bytes = vframe.width() as usize;
                    let dest_stride = vframe.plane_stride()[0] as usize;
                    let dest = vframe.plane_data_mut(0).unwrap();
                    let src_stride = video_frame.line_stride_or_data_size_in_bytes() as usize;
                    let src = video_frame.data();

                    for (dest, src) in dest
                        .chunks_exact_mut(dest_stride)
                        .zip(src.chunks_exact(src_stride))
                    {
                        dest.copy_from_slice(&src[..line_bytes]);
                    }
                }

                // Second plane
                {
                    let line_bytes = vframe.width() as usize;
                    let dest_stride = vframe.plane_stride()[1] as usize;
                    let dest = vframe.plane_data_mut(1).unwrap();
                    let src_stride = video_frame.line_stride_or_data_size_in_bytes() as usize;
                    let src = &video_frame.data()[(video_frame.yres() as usize * src_stride)..];

                    for (dest, src) in dest
                        .chunks_exact_mut(dest_stride)
                        .zip(src.chunks_exact(src_stride))
                    {
                        dest.copy_from_slice(&src[..line_bytes]);
                    }
                }
            }
            gst_video::VideoFormat::Yv12 | gst_video::VideoFormat::I420 => {
                // First plane
                {
                    let line_bytes = vframe.width() as usize;
                    let dest_stride = vframe.plane_stride()[0] as usize;
                    let dest = vframe.plane_data_mut(0).unwrap();
                    let src_stride = video_frame.line_stride_or_data_size_in_bytes() as usize;
                    let src = video_frame.data();

                    for (dest, src) in dest
                        .chunks_exact_mut(dest_stride)
                        .zip(src.chunks_exact(src_stride))
                    {
                        dest.copy_from_slice(&src[..line_bytes]);
                    }
                }

                // Second plane
                {
                    let line_bytes = (vframe.width() as usize + 1) / 2;
                    let dest_stride = vframe.plane_stride()[1] as usize;
                    let dest = vframe.plane_data_mut(1).unwrap();
                    let src_stride = video_frame.line_stride_or_data_size_in_bytes() as usize;
                    let src_stride1 = video_frame.line_stride_or_data_size_in_bytes() as usize / 2;
                    let src = &video_frame.data()[(video_frame.yres() as usize * src_stride)..];

                    for (dest, src) in dest
                        .chunks_exact_mut(dest_stride)
                        .zip(src.chunks_exact(src_stride1))
                    {
                        dest.copy_from_slice(&src[..line_bytes]);
                    }
                }

                // Third plane
                {
                    let line_bytes = (vframe.width() as usize + 1) / 2;
                    let dest_stride = vframe.plane_stride()[2] as usize;
                    let dest = vframe.plane_data_mut(2).unwrap();
                    let src_stride = video_frame.line_stride_or_data_size_in_bytes() as usize;
                    let src_stride1 = video_frame.line_stride_or_data_size_in_bytes() as usize / 2;
                    let src = &video_frame.data()[(video_frame.yres() as usize * src_stride
                        + (video_frame.yres() as usize + 1) / 2 * src_stride1)..];

                    for (dest, src) in dest
                        .chunks_exact_mut(dest_stride)
                        .zip(src.chunks_exact(src_stride1))
                    {
                        dest.copy_from_slice(&src[..line_bytes]);
                    }
                }
            }
            _ => unreachable!(),
        }

        vframe.into_buffer()
    }

    fn create_audio_buffer_and_info(
        &self,
        element: &gst_base::BaseSrc,
        audio_frame: AudioFrame,
    ) -> Result<Buffer, gst::FlowError> {
        gst_debug!(CAT, obj: element, "Received audio frame {:?}", audio_frame);

        let (pts, duration) = self
            .calculate_audio_timestamp(element, &audio_frame)
            .ok_or_else(|| {
                gst_debug!(CAT, obj: element, "Flushing, dropping buffer");
                gst::FlowError::Flushing
            })?;

        let info = self.create_audio_info(element, &audio_frame)?;

        let buffer = self.create_audio_buffer(element, pts, duration, &info, &audio_frame);

        gst_log!(CAT, obj: element, "Produced audio buffer {:?}", buffer);

        Ok(Buffer::Audio(buffer, info))
    }

    fn calculate_audio_timestamp(
        &self,
        element: &gst_base::BaseSrc,
        audio_frame: &AudioFrame,
    ) -> Option<(gst::ClockTime, Option<gst::ClockTime>)> {
        let duration = gst::ClockTime::SECOND.mul_div_floor(
            audio_frame.no_samples() as u64,
            audio_frame.sample_rate() as u64,
        );

        self.calculate_timestamp(
            element,
            audio_frame.timestamp(),
            audio_frame.timecode(),
            duration,
        )
    }

    fn create_audio_info(
        &self,
        element: &gst_base::BaseSrc,
        audio_frame: &AudioFrame,
    ) -> Result<gst_audio::AudioInfo, gst::FlowError> {
        let builder = gst_audio::AudioInfo::builder(
            gst_audio::AUDIO_FORMAT_S16,
            audio_frame.sample_rate() as u32,
            audio_frame.no_channels() as u32,
        );

        builder.build().map_err(|_| {
            gst::element_error!(
                element,
                gst::StreamError::Format,
                ["Invalid audio format configuration"]
            );

            gst::FlowError::NotNegotiated
        })
    }

    fn create_audio_buffer(
        &self,
        _element: &gst_base::BaseSrc,
        pts: gst::ClockTime,
        duration: Option<gst::ClockTime>,
        info: &gst_audio::AudioInfo,
        audio_frame: &AudioFrame,
    ) -> gst::Buffer {
        // We multiply by 2 because is the size in bytes of an i16 variable
        let buff_size = (audio_frame.no_samples() as u32 * info.bpf()) as usize;
        let mut buffer = gst::Buffer::with_size(buff_size).unwrap();
        {
            let buffer = buffer.get_mut().unwrap();

            buffer.set_pts(pts);
            buffer.set_duration(duration);

            #[cfg(feature = "reference-timestamps")]
            {
                gst::ReferenceTimestampMeta::add(
                    buffer,
                    &*TIMECODE_CAPS,
                    gst::ClockTime::from_nseconds(audio_frame.timecode() as u64 * 100),
                    gst::ClockTime::NONE,
                );
                if audio_frame.timestamp() != ndisys::NDIlib_recv_timestamp_undefined {
                    gst::ReferenceTimestampMeta::add(
                        buffer,
                        &*TIMESTAMP_CAPS,
                        gst::ClockTime::from_nseconds(audio_frame.timestamp() as u64 * 100),
                        gst::ClockTime::NONE,
                    );
                }
            }

            audio_frame.copy_to_interleaved_16s(
                buffer
                    .map_writable()
                    .unwrap()
                    .as_mut_slice_of::<i16>()
                    .unwrap(),
            );
        }

        buffer
    }
}
