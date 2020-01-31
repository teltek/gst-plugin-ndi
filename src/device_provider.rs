use glib;
use glib::subclass;
use gst;
use gst::prelude::*;
use gst::subclass::prelude::*;

use once_cell::sync::OnceCell;

use std::sync::atomic;
use std::sync::Mutex;
use std::thread;

use crate::ndi;

#[derive(Debug)]
struct DeviceProvider {
    cat: gst::DebugCategory,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
    current_devices: Mutex<Vec<gst::Device>>,
    find: Mutex<Option<ndi::FindInstance>>,
    is_running: atomic::AtomicBool,
}

impl ObjectSubclass for DeviceProvider {
    const NAME: &'static str = "NdiDeviceProvider";
    type ParentType = gst::DeviceProvider;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            cat: gst::DebugCategory::new(
                "ndideviceprovider",
                gst::DebugColorFlags::empty(),
                Some("NewTek NDI Device Provider"),
            ),
            thread: Mutex::new(None),
            current_devices: Mutex::new(vec![]),
            find: Mutex::new(None),
            is_running: atomic::AtomicBool::new(false),
        }
    }

    fn class_init(klass: &mut subclass::simple::ClassStruct<Self>) {
        klass.set_metadata(
            "NewTek NDI Device Provider",
            "Source/Audio/Video/Network",
            "NewTek NDI Device Provider",
            "Ruben Gonzalez <rubenrua@teltek.es>, Daniel Vilar <daniel.peiteado@teltek.es>, Sebastian Dröge <sebastian@centricular.com>",
        );
    }
}

impl ObjectImpl for DeviceProvider {
    glib_object_impl!();
}

impl DeviceProviderImpl for DeviceProvider {
    fn probe(&self, _device_provider: &gst::DeviceProvider) -> Vec<gst::Device> {
        self.current_devices.lock().unwrap().clone()
    }
    fn start(&self, device_provider: &gst::DeviceProvider) -> Result<(), gst::LoggableError> {
        let mut thread_guard = self.thread.lock().unwrap();
        if thread_guard.is_some() {
            gst_log!(
                self.cat,
                obj: device_provider,
                "Device provider already started"
            );
            return Ok(());
        }

        self.is_running.store(true, atomic::Ordering::SeqCst);

        let device_provider_weak = device_provider.downgrade();
        let mut first = true;
        *thread_guard = Some(thread::spawn(move || {
            let device_provider = match device_provider_weak.upgrade() {
                None => return,
                Some(device_provider) => device_provider,
            };

            let imp = DeviceProvider::from_instance(&device_provider);
            {
                let mut find_guard = imp.find.lock().unwrap();
                if find_guard.is_some() {
                    gst_log!(imp.cat, obj: &device_provider, "Already started");
                    return;
                }

                let find = match ndi::FindInstance::builder().build() {
                    None => {
                        gst_error!(
                            imp.cat,
                            obj: &device_provider,
                            "Failed to create Find instance"
                        );
                        return;
                    }
                    Some(find) => find,
                };
                *find_guard = Some(find);
            }

            loop {
                let device_provider = match device_provider_weak.upgrade() {
                    None => break,
                    Some(device_provider) => device_provider,
                };

                let imp = DeviceProvider::from_instance(&device_provider);
                if !imp.is_running.load(atomic::Ordering::SeqCst) {
                    break;
                }

                imp.poll(&device_provider, first);
                first = false;
            }
        }));

        Ok(())
    }
    fn stop(&self, _device_provider: &gst::DeviceProvider) {
        if let Some(_thread) = self.thread.lock().unwrap().take() {
            self.is_running.store(false, atomic::Ordering::SeqCst);
            // Don't actually join because that might take a while
        }
    }
}

impl DeviceProvider {
    fn poll(&self, device_provider: &gst::DeviceProvider, first: bool) {
        let mut find_guard = self.find.lock().unwrap();
        let find = match *find_guard {
            None => return,
            Some(ref mut find) => find,
        };

        if !find.wait_for_sources(if first { 1000 } else { 5000 }) {
            gst_trace!(self.cat, obj: device_provider, "No new sources found");
            return;
        }

        let sources = find.get_current_sources();
        let mut sources = sources.iter().map(|s| s.to_owned()).collect::<Vec<_>>();

        let mut current_devices_guard = self.current_devices.lock().unwrap();
        let mut expired_devices = vec![];
        let mut remaining_sources = vec![];

        // First check for each device we previously knew if it's still available
        for old_device in &*current_devices_guard {
            let old_device_imp = Device::from_instance(old_device);
            let old_source = old_device_imp.source.get().unwrap();

            if !sources.contains(&old_source.0) {
                gst_log!(
                    self.cat,
                    obj: device_provider,
                    "Source {:?} disappeared",
                    old_source
                );
                expired_devices.push(old_device.clone());
            } else {
                // Otherwise remember that we had it before already and don't have to announce it
                // again. After the loop we're going to remove these all from the sources vec.
                remaining_sources.push(old_source.0.to_owned());
            }
        }

        for remaining_source in remaining_sources {
            sources.retain(|s| s != &remaining_source);
        }

        // Remove all expired devices from the list of cached devices
        current_devices_guard.retain(|d| !expired_devices.contains(d));
        // And also notify the device provider of them having disappeared
        for old_device in expired_devices {
            device_provider.device_remove(&old_device);
        }

        // Now go through all new devices and announce them
        for source in sources {
            gst_log!(
                self.cat,
                obj: device_provider,
                "Source {:?} appeared",
                source
            );
            // Add once for audio, another time for video
            let device = Device::new(&source, true);
            device_provider.device_add(&device);
            current_devices_guard.push(device);

            let device = Device::new(&source, false);
            device_provider.device_add(&device);
            current_devices_guard.push(device);
        }
    }
}

#[derive(Debug)]
struct Device {
    cat: gst::DebugCategory,
    source: OnceCell<(ndi::Source<'static>, glib::Type)>,
}

impl ObjectSubclass for Device {
    const NAME: &'static str = "NdiDevice";
    type ParentType = gst::Device;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            cat: gst::DebugCategory::new(
                "ndidevice",
                gst::DebugColorFlags::empty(),
                Some("NewTek NDI Device"),
            ),
            source: OnceCell::new(),
        }
    }
}

impl ObjectImpl for Device {
    glib_object_impl!();
}

impl DeviceImpl for Device {
    fn create_element(
        &self,
        _device: &gst::Device,
        name: Option<&str>,
    ) -> Result<gst::Element, gst::LoggableError> {
        let source_info = self.source.get().unwrap();
        let element = glib::Object::new(
            source_info.1,
            &[
                ("name", &name),
                ("ndi-name", &source_info.0.ndi_name()),
                ("url-address", &source_info.0.url_address()),
            ],
        )
        .unwrap()
        .dynamic_cast::<gst::Element>()
        .unwrap();

        Ok(element)
    }
}

impl Device {
    fn new(source: &ndi::Source<'_>, is_audio: bool) -> gst::Device {
        let display_name = format!(
            "{} ({})",
            source.ndi_name(),
            if is_audio { "Audio" } else { "Video" }
        );
        let device_class = format!(
            "Source/{}/Network",
            if is_audio { "Audio" } else { "Video" }
        );

        // Get the caps from the template caps of the corresponding source element
        let element_type = if is_audio {
            crate::ndiaudiosrc::NdiAudioSrc::get_type()
        } else {
            crate::ndivideosrc::NdiVideoSrc::get_type()
        };
        let element_class = gst::ElementClass::from_type(element_type).unwrap();
        let templ = element_class.get_pad_template("src").unwrap();
        let caps = templ.get_caps().unwrap();

        // Put the url-address into the extra properties
        let extra_properties = gst::Structure::builder("properties")
            .field("ndi-name", &source.ndi_name())
            .field("url-address", &source.url_address())
            .build();

        let device = glib::Object::new(
            Device::get_type(),
            &[
                ("caps", &caps),
                ("display-name", &display_name),
                ("device-class", &device_class),
                ("properties", &extra_properties),
            ],
        )
        .unwrap()
        .dynamic_cast::<gst::Device>()
        .unwrap();
        let device_impl = Device::from_instance(&device);

        device_impl
            .source
            .set((source.to_owned(), element_type))
            .unwrap();

        device
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::DeviceProvider::register(
        Some(plugin),
        "ndideviceprovider",
        gst::Rank::Primary,
        DeviceProvider::get_type(),
    )
}
