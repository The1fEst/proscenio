use std::cell::RefCell;
use std::rc::Rc;

use crate::services::media::audio::{Audio, Device};
use crate::ui::widgets::input::controls::ComboBox;

pub struct DevicePicker {
    is_sink: bool,
    audio: Audio,
    devices: RefCell<Vec<Device>>,
    combo: Rc<ComboBox>,
    on_updated: RefCell<Option<Rc<dyn Fn()>>>,
}

impl DevicePicker {
    pub fn new(combo: Rc<ComboBox>, audio: &Audio, is_sink: bool) -> Rc<Self> {
        let picker = Rc::new(DevicePicker {
            is_sink,
            audio: audio.clone(),
            devices: RefCell::new(Vec::new()),
            combo,
            on_updated: RefCell::new(None),
        });
        picker.combo.connect_activated({
            let picker = Rc::downgrade(&picker);
            move |index| {
                let Some(picker) = picker.upgrade() else {
                    return;
                };
                if let Some(device) = picker.devices.borrow().get(index) {
                    picker.audio.set_default(picker.is_sink, &device.name);
                }
            }
        });
        picker
    }

    pub fn connect_updated(&self, action: impl Fn() + 'static) {
        self.on_updated.replace(Some(Rc::new(action)));
    }

    pub fn refresh(self: &Rc<Self>) {
        let picker = Rc::downgrade(self);
        self.audio
            .read_devices(self.is_sink, move |devices, current_name| {
                let Some(picker) = picker.upgrade() else {
                    return;
                };
                let labels: Vec<String> = devices.iter().map(device_label).collect();
                let index = resolve_current_index(&devices, &current_name);
                let has_changed = picker.combo.set_items(&labels, index as i32);
                picker.devices.replace(devices);
                let on_updated = picker.on_updated.borrow().clone();
                if let (true, Some(on_updated)) = (has_changed, on_updated) {
                    on_updated();
                }
            });
    }
}

fn device_label(device: &Device) -> String {
    let (description, nick) = (&device.description, &device.nick);
    if !nick.is_empty() && description.len() > nick.len() && description.starts_with(nick.as_str())
    {
        return format!("{nick} · {}", description[nick.len()..].trim());
    }
    [description, nick, &device.name]
        .into_iter()
        .find(|candidate| !candidate.is_empty())
        .cloned()
        .unwrap_or_default()
}

fn resolve_current_index(devices: &[Device], current_name: &str) -> usize {
    devices
        .iter()
        .position(|device| device.name == current_name)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, description: &str, nick: &str) -> Device {
        Device {
            name: name.to_owned(),
            description: description.to_owned(),
            nick: nick.to_owned(),
        }
    }

    #[test]
    fn a_label_splits_off_the_nick_the_description_starts_with() {
        for (device, label) in [
            (
                device("alsa_output.pci", "Speakers Analog Stereo", "Speakers"),
                "Speakers · Analog Stereo",
            ),
            (
                device("alsa_output.pci", "Speakers", "Speakers"),
                "Speakers",
            ),
            (device("alsa_output.pci", "Headset", "Speakers"), "Headset"),
            (device("alsa_output.pci", "", "Speakers"), "Speakers"),
            (device("alsa_output.pci", "", ""), "alsa_output.pci"),
        ] {
            assert_eq!(device_label(&device), label);
        }
    }

    #[test]
    fn the_current_device_is_the_one_named_by_the_server_else_the_first() {
        let devices = [
            device("a", "", ""),
            device("b", "", ""),
            device("c", "", ""),
        ];
        assert_eq!(resolve_current_index(&devices, "b"), 1);
        assert_eq!(resolve_current_index(&devices, "missing"), 0);
        assert_eq!(resolve_current_index(&[], "a"), 0);
    }
}
