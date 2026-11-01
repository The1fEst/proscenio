use gtk4::gio;
use gtk4::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

pub const SYSFS_CLASS: &str = "/sys/class";
pub const BACKLIGHT: &str = "backlight";
const BACKLIGHT_TYPES: [&str; 3] = ["firmware", "platform", "raw"];
const LEDS: &str = "leds";
const KEYBOARD_FUNCTION: &str = "kbd_backlight";
const LOGIN: &str = "org.freedesktop.login1";
const SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";
const SESSION: &str = "org.freedesktop.login1.Session";
const TIMEOUT_MILLIS: i32 = 2000;

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    subsystem: &'static str,
    name: String,
    path: PathBuf,
}

impl Device {
    fn new(subsystem: &'static str, directory: &Path, name: String) -> Self {
        Device {
            subsystem,
            path: directory.join(&name),
            name,
        }
    }

    pub fn read_brightness(&self) -> Option<u32> {
        read_number(&self.path.join("brightness"))
    }

    pub fn read_actual_brightness(&self) -> Option<u32> {
        read_number(&self.path.join("actual_brightness")).or_else(|| self.read_brightness())
    }

    pub fn read_max_brightness(&self) -> Option<u32> {
        read_number(&self.path.join("max_brightness")).filter(|max| *max > 0)
    }

    pub fn set_brightness(
        &self,
        connection: &gio::DBusConnection,
        brightness: u32,
        on_done: impl FnOnce() + 'static,
    ) {
        let name = self.name.clone();
        connection.call(
            Some(LOGIN),
            SESSION_PATH,
            SESSION,
            "SetBrightness",
            Some(&(self.subsystem, self.name.as_str(), brightness).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT_MILLIS,
            gio::Cancellable::NONE,
            move |result| {
                if let Err(error) = result {
                    eprintln!("brightness: logind did not set {name} to {brightness}: {error}");
                }
                on_done();
            },
        );
    }
}

pub fn find_screen_backlight(class_root: &Path) -> Option<Device> {
    let directory = class_root.join(BACKLIGHT);
    list_names(&directory)
        .into_iter()
        .min_by_key(|name| {
            let kind = fs::read_to_string(directory.join(name).join("type")).unwrap_or_default();
            let rank = BACKLIGHT_TYPES
                .iter()
                .position(|known| *known == kind.trim())
                .unwrap_or(BACKLIGHT_TYPES.len());
            (rank, name.clone())
        })
        .map(|name| Device::new(BACKLIGHT, &directory, name))
}

pub fn find_keyboard_backlight(class_root: &Path) -> Option<Device> {
    let directory = class_root.join(LEDS);
    list_names(&directory)
        .into_iter()
        .filter(|name| {
            name.rsplit_once(':')
                .is_some_and(|(_, function)| function == KEYBOARD_FUNCTION)
        })
        .min()
        .map(|name| Device::new(LEDS, &directory, name))
}

fn list_names(directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

fn read_number(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ClassRoot(PathBuf);

    impl ClassRoot {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("proscenio-sysfs-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            ClassRoot(path)
        }

        fn add(&self, subsystem: &str, name: &str, attributes: &[(&str, &str)]) {
            let device = self.0.join(subsystem).join(name);
            fs::create_dir_all(&device).unwrap();
            for (attribute, value) in attributes {
                fs::write(device.join(attribute), format!("{value}\n")).unwrap();
            }
        }
    }

    impl Drop for ClassRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn find_screen_backlight_name(root: &ClassRoot) -> Option<String> {
        find_screen_backlight(&root.0).map(|device| device.name)
    }

    #[test]
    fn the_screen_backlight_prefers_firmware_then_platform_then_raw() {
        let root = ClassRoot::new("choice");
        root.add(BACKLIGHT, "intel_backlight", &[("type", "raw")]);
        root.add(BACKLIGHT, "acpi_video0", &[("type", "firmware")]);
        root.add(BACKLIGHT, "dell_backlight", &[("type", "platform")]);
        assert_eq!(
            find_screen_backlight_name(&root).as_deref(),
            Some("acpi_video0")
        );
        fs::remove_dir_all(root.0.join(BACKLIGHT).join("acpi_video0")).unwrap();
        assert_eq!(
            find_screen_backlight_name(&root).as_deref(),
            Some("dell_backlight")
        );
    }

    #[test]
    fn backlights_of_one_type_go_by_name_and_none_is_found_without_the_class() {
        let root = ClassRoot::new("names");
        assert_eq!(find_screen_backlight(&root.0), None);
        root.add(BACKLIGHT, "nvidia_1", &[("type", "raw")]);
        root.add(BACKLIGHT, "nvidia_0", &[("type", "raw")]);
        assert_eq!(
            find_screen_backlight_name(&root).as_deref(),
            Some("nvidia_0")
        );
    }

    #[test]
    fn the_level_comes_from_actual_brightness_when_the_device_has_it() {
        let root = ClassRoot::new("read");
        root.add(
            BACKLIGHT,
            "amdgpu_bl1",
            &[
                ("brightness", "120"),
                ("actual_brightness", "118"),
                ("max_brightness", "255"),
            ],
        );
        let device = find_screen_backlight(&root.0).unwrap();
        assert_eq!(
            (
                device.read_actual_brightness(),
                device.read_brightness(),
                device.read_max_brightness()
            ),
            (Some(118), Some(120), Some(255))
        );
        fs::remove_file(root.0.join(BACKLIGHT).join("amdgpu_bl1/actual_brightness")).unwrap();
        fs::write(
            root.0.join(BACKLIGHT).join("amdgpu_bl1/max_brightness"),
            "0",
        )
        .unwrap();
        assert_eq!(
            (
                device.read_actual_brightness(),
                device.read_max_brightness()
            ),
            (Some(120), None)
        );
    }

    #[test]
    fn only_a_led_whose_function_is_kbd_backlight_is_the_keyboard_backlight() {
        let root = ClassRoot::new("leds");
        root.add(LEDS, "input3::capslock", &[("brightness", "0")]);
        root.add(LEDS, "tpacpi::kbd_backlight_extra", &[("brightness", "0")]);
        assert_eq!(find_keyboard_backlight(&root.0), None);
        root.add(
            LEDS,
            "tpacpi::kbd_backlight",
            &[("brightness", "1"), ("max_brightness", "2")],
        );
        root.add(LEDS, "rgb:kbd_backlight", &[("brightness", "0")]);
        let device = find_keyboard_backlight(&root.0).unwrap();
        assert_eq!(
            (
                device.name.as_str(),
                device.subsystem,
                device.read_brightness()
            ),
            ("rgb:kbd_backlight", LEDS, Some(0))
        );
    }
}
