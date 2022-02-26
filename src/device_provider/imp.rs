use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::{gst_error, gst_log, gst_trace};

use once_cell::sync::OnceCell;

use std::sync::atomic;
use std::sync::Mutex;
use std::thread;

use once_cell::sync::Lazy;

use crate::ndi;

static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "ndideviceprovider",
        gst::DebugColorFlags::empty(),
        Some("NewTek NDI Device Provider"),
    )
});

#[derive(Debug)]
pub struct DeviceProvider {
    thread: Mutex<Option<thread::JoinHandle<()>>>,
    current_devices: Mutex<Vec<super::Device>>,
    find: Mutex<Option<ndi::FindInstance>>,
    is_running: atomic::AtomicBool,
}

#[glib::object_subclass]
impl ObjectSubclass for DeviceProvider {
    const NAME: &'static str = "NdiDeviceProvider";
    type Type = super::DeviceProvider;
    type ParentType = gst::DeviceProvider;

    fn new() -> Self {
        Self {
            thread: Mutex::new(None),
            current_devices: Mutex::new(vec![]),
            find: Mutex::new(None),
            is_running: atomic::AtomicBool::new(false),
        }
    }
}

impl ObjectImpl for DeviceProvider {}

impl GstObjectImpl for DeviceProvider {}

impl DeviceProviderImpl for DeviceProvider {
    fn metadata() -> Option<&'static gst::subclass::DeviceProviderMetadata> {
        static METADATA: Lazy<gst::subclass::DeviceProviderMetadata> = Lazy::new(|| {
            gst::subclass::DeviceProviderMetadata::new("NewTek NDI Device Provider",
            "Source/Audio/Video/Network",
            "NewTek NDI Device Provider",
            "Ruben Gonzalez <rubenrua@teltek.es>, Daniel Vilar <daniel.peiteado@teltek.es>, Sebastian Dröge <sebastian@centricular.com>")
        });

        Some(&*METADATA)
    }

    fn probe(&self, _device_provider: &Self::Type) -> Vec<gst::Device> {
        self.current_devices
            .lock()
            .unwrap()
            .iter()
            .map(|d| d.clone().upcast())
            .collect()
    }

    fn start(&self, device_provider: &Self::Type) -> Result<(), gst::LoggableError> {
        let mut thread_guard = self.thread.lock().unwrap();
        if thread_guard.is_some() {
            gst_log!(CAT, obj: device_provider, "Device provider already started");
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
                    gst_log!(CAT, obj: &device_provider, "Already started");
                    return;
                }

                let find = match ndi::FindInstance::builder().build() {
                    None => {
                        gst_error!(CAT, obj: &device_provider, "Failed to create Find instance");
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

    fn stop(&self, _device_provider: &Self::Type) {
        if let Some(_thread) = self.thread.lock().unwrap().take() {
            self.is_running.store(false, atomic::Ordering::SeqCst);
            // Don't actually join because that might take a while
        }
    }
}

impl DeviceProvider {
    fn poll(&self, device_provider: &super::DeviceProvider, first: bool) {
        let mut find_guard = self.find.lock().unwrap();
        let find = match *find_guard {
            None => return,
            Some(ref mut find) => find,
        };

        if !find.wait_for_sources(if first { 1000 } else { 5000 }) {
            gst_trace!(CAT, obj: device_provider, "No new sources found");
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

            if !sources.contains(&*old_source) {
                gst_log!(
                    CAT,
                    obj: device_provider,
                    "Source {:?} disappeared",
                    old_source
                );
                expired_devices.push(old_device.clone());
            } else {
                // Otherwise remember that we had it before already and don't have to announce it
                // again. After the loop we're going to remove these all from the sources vec.
                remaining_sources.push(old_source.to_owned());
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
            gst_log!(CAT, obj: device_provider, "Source {:?} appeared", source);
            let device = super::Device::new(&source);
            device_provider.device_add(&device);
            current_devices_guard.push(device);
        }
    }
}

#[derive(Debug)]
pub struct Device {
    source: OnceCell<ndi::Source<'static>>,
}

#[glib::object_subclass]
impl ObjectSubclass for Device {
    const NAME: &'static str = "NdiDevice";
    type Type = super::Device;
    type ParentType = gst::Device;

    fn new() -> Self {
        Self {
            source: OnceCell::new(),
        }
    }
}

impl ObjectImpl for Device {}

impl GstObjectImpl for Device {}

impl DeviceImpl for Device {
    fn create_element(
        &self,
        _device: &Self::Type,
        name: Option<&str>,
    ) -> Result<gst::Element, gst::LoggableError> {
        let source_info = self.source.get().unwrap();
        let element = glib::Object::with_type(
            crate::ndisrc::NdiSrc::static_type(),
            &[
                ("name", &name),
                ("ndi-name", &source_info.ndi_name()),
                ("url-address", &source_info.url_address()),
            ],
        )
        .unwrap()
        .dynamic_cast::<gst::Element>()
        .unwrap();

        Ok(element)
    }
}

impl super::Device {
    fn new(source: &ndi::Source<'_>) -> super::Device {
        let display_name = source.ndi_name();
        let device_class = "Source/Audio/Video/Network";

        let element_class =
            glib::Class::<gst::Element>::from_type(crate::ndisrc::NdiSrc::static_type()).unwrap();
        let templ = element_class.pad_template("src").unwrap();
        let caps = templ.caps();

        // Put the url-address into the extra properties
        let extra_properties = gst::Structure::builder("properties")
            .field("ndi-name", &source.ndi_name())
            .field("url-address", &source.url_address())
            .build();

        let device = glib::Object::new::<super::Device>(&[
            ("caps", &caps),
            ("display-name", &display_name),
            ("device-class", &device_class),
            ("properties", &extra_properties),
        ])
        .unwrap();
        let device_impl = Device::from_instance(&device);

        device_impl.source.set(source.to_owned()).unwrap();

        device
    }
}
