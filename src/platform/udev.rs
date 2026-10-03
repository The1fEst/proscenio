use std::ffi::{CStr, CString, c_char, c_int, c_void};

#[link(name = "udev")]
unsafe extern "C" {
    fn udev_new() -> *mut c_void;
    fn udev_unref(udev: *mut c_void) -> *mut c_void;
    fn udev_monitor_new_from_netlink(udev: *mut c_void, name: *const c_char) -> *mut c_void;
    fn udev_monitor_filter_add_match_subsystem_devtype(
        monitor: *mut c_void,
        subsystem: *const c_char,
        devtype: *const c_char,
    ) -> c_int;
    fn udev_monitor_enable_receiving(monitor: *mut c_void) -> c_int;
    fn udev_monitor_get_fd(monitor: *mut c_void) -> c_int;
    fn udev_monitor_receive_device(monitor: *mut c_void) -> *mut c_void;
    fn udev_monitor_unref(monitor: *mut c_void) -> *mut c_void;
    fn udev_device_unref(device: *mut c_void) -> *mut c_void;
    fn udev_device_get_action(device: *mut c_void) -> *const c_char;
    fn udev_device_get_syspath(device: *mut c_void) -> *const c_char;
    fn udev_device_get_devtype(device: *mut c_void) -> *const c_char;
    fn udev_device_get_property_value(device: *mut c_void, key: *const c_char) -> *const c_char;
    fn udev_device_get_sysattr_value(device: *mut c_void, name: *const c_char) -> *const c_char;
}

const GENERIC: &str = "Generic";

pub struct Monitor {
    udev: *mut c_void,
    monitor: *mut c_void,
}

impl Monitor {
    pub fn subsystem(subsystem: &str) -> Option<Monitor> {
        let subsystem = CString::new(subsystem).ok()?;
        let udev = unsafe { udev_new() };
        if udev.is_null() {
            return None;
        }
        let monitor = unsafe { udev_monitor_new_from_netlink(udev, c"udev".as_ptr()) };
        let created = Monitor { udev, monitor };
        if monitor.is_null() {
            return None;
        }
        unsafe {
            udev_monitor_filter_add_match_subsystem_devtype(
                monitor,
                subsystem.as_ptr(),
                std::ptr::null(),
            );
            udev_monitor_enable_receiving(monitor);
        }
        Some(created)
    }

    pub fn fd(&self) -> i32 {
        unsafe { udev_monitor_get_fd(self.monitor) }
    }

    pub fn receive(&self) -> Option<Device> {
        let device = unsafe { udev_monitor_receive_device(self.monitor) };
        (!device.is_null()).then_some(Device { device })
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        unsafe {
            if !self.monitor.is_null() {
                udev_monitor_unref(self.monitor);
            }
            udev_unref(self.udev);
        }
    }
}

pub struct Device {
    device: *mut c_void,
}

impl Device {
    fn text(value: *const c_char) -> Option<String> {
        if value.is_null() {
            return None;
        }
        Some(
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned(),
        )
    }

    pub fn action(&self) -> String {
        Self::text(unsafe { udev_device_get_action(self.device) }).unwrap_or_default()
    }

    pub fn sysfs_path(&self) -> String {
        Self::text(unsafe { udev_device_get_syspath(self.device) }).unwrap_or_default()
    }

    pub fn device_type(&self) -> String {
        Self::text(unsafe { udev_device_get_devtype(self.device) }).unwrap_or_default()
    }

    fn property_raw(&self, name: &str) -> Option<Vec<u8>> {
        let name = CString::new(name).ok()?;
        let value = unsafe { udev_device_get_property_value(self.device, name.as_ptr()) };
        if value.is_null() {
            return None;
        }
        Some(unsafe { CStr::from_ptr(value) }.to_bytes().to_vec())
    }

    fn property(&self, name: &str) -> String {
        self.property_raw(name)
            .map(|value| String::from_utf8_lossy(&value).into_owned())
            .unwrap_or_default()
    }

    fn sysfs_property(&self, name: &str) -> String {
        let Ok(name) = CString::new(name) else {
            return String::new();
        };
        Self::text(unsafe { udev_device_get_sysattr_value(self.device, name.as_ptr()) })
            .unwrap_or_default()
    }

    fn named(&self, attribute: &str, database: &str, encoded: &str, plain: &str) -> String {
        let mut name = self.sysfs_property(attribute);
        if name.is_empty() {
            name = self.property(database);
        }
        if name.is_empty() {
            name = decode_property_value(&self.property_raw(encoded).unwrap_or_default());
        }
        if name.is_empty() {
            name = self.property(plain);
        }
        name
    }

    pub fn model(&self) -> String {
        self.named(
            "product",
            "ID_MODEL_FROM_DATABASE",
            "ID_MODEL_ENC",
            "ID_MODEL",
        )
    }

    pub fn vendor(&self) -> String {
        self.named(
            "manufacturer",
            "ID_VENDOR_FROM_DATABASE",
            "ID_VENDOR_ENC",
            "ID_VENDOR",
        )
    }

    pub fn display_name(&self) -> String {
        display_name(&self.vendor(), &self.model())
    }

    pub fn is_removable(&self) -> bool {
        self.sysfs_property("removable") == "removable"
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            udev_device_unref(self.device);
        }
    }
}

fn display_name(vendor: &str, model: &str) -> String {
    [vendor, model]
        .into_iter()
        .filter(|part| !part.is_empty() && *part != GENERIC)
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_property_value(encoded: &[u8]) -> String {
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        let byte = encoded[index];
        if byte != b'\\' {
            decoded.push(byte);
        } else if encoded.get(index + 1) == Some(&b'\\') {
            decoded.push(b'\\');
            index += 1;
        } else if index + 3 < encoded.len() && encoded[index + 1] == b'x' {
            let code = std::str::from_utf8(&encoded[index + 2..index + 4])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            if let Some(code) = code {
                decoded.push(code);
            }
            index += 3;
        }
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udev_encoded_names_decode_as_kde_decodes_them() {
        assert_eq!(
            decode_property_value(b"USB\\x20Flash\\x20Disk"),
            "USB Flash Disk"
        );
        assert_eq!(decode_property_value(b"a\\\\b"), "a\\b");
        assert_eq!(decode_property_value(b"\\xd0\\x9f"), "П");
        assert_eq!(decode_property_value(b"end\\x2"), "endx2");
    }

    #[test]
    fn a_device_name_skips_generic_and_empty_parts() {
        assert_eq!(display_name("SanDisk", "Cruzer"), "SanDisk Cruzer");
        assert_eq!(display_name("Generic", "Mass Storage"), "Mass Storage");
        assert_eq!(display_name("", "Generic"), "");
    }
}
