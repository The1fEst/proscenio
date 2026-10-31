use std::cell::RefCell;
use std::rc::Rc;

use crate::services::media::audio::{Audio, Device};
use crate::ui::widgets::input::controls::ComboBox;

struct Choice {
    name: String,
    port: Option<String>,
    label: String,
    is_port_active: bool,
}

pub struct DevicePicker {
    is_sink: bool,
    audio: Audio,
    choices: RefCell<Vec<Choice>>,
    combo: Rc<ComboBox>,
    on_updated: RefCell<Option<Rc<dyn Fn()>>>,
}

impl DevicePicker {
    pub fn new(combo: Rc<ComboBox>, audio: &Audio, is_sink: bool) -> Rc<Self> {
        let picker = Rc::new(DevicePicker {
            is_sink,
            audio: audio.clone(),
            choices: RefCell::new(Vec::new()),
            combo,
            on_updated: RefCell::new(None),
        });
        picker.combo.connect_activated({
            let picker = Rc::downgrade(&picker);
            move |index| {
                let Some(picker) = picker.upgrade() else {
                    return;
                };
                let choices = picker.choices.borrow();
                let Some(choice) = choices.get(index) else {
                    return;
                };
                if let Some(port) = &choice.port {
                    picker.audio.set_sink_port(&choice.name, port);
                }
                picker.audio.set_default(picker.is_sink, &choice.name);
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
                let choices = list_choices(&devices);
                let labels: Vec<String> =
                    choices.iter().map(|choice| choice.label.clone()).collect();
                let index = resolve_current_index(&choices, &current_name);
                let has_changed = picker.combo.set_items(&labels, index as i32);
                picker.choices.replace(choices);
                let on_updated = picker.on_updated.borrow().clone();
                if let (true, Some(on_updated)) = (has_changed, on_updated) {
                    on_updated();
                }
            });
    }
}

fn list_choices(devices: &[Device]) -> Vec<Choice> {
    let mut choices = Vec::new();
    for device in devices {
        let label = device_label(device);
        if device.ports.len() < 2 {
            choices.push(Choice {
                name: device.name.clone(),
                port: None,
                label,
                is_port_active: true,
            });
            continue;
        }
        for port in &device.ports {
            choices.push(Choice {
                name: device.name.clone(),
                port: Some(port.name.clone()),
                label: format!("{label} · {}", port.description),
                is_port_active: port.name == device.active_port,
            });
        }
    }
    choices
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

fn resolve_current_index(choices: &[Choice], current_name: &str) -> usize {
    choices
        .iter()
        .position(|choice| choice.name == current_name && choice.is_port_active)
        .or_else(|| {
            choices
                .iter()
                .position(|choice| choice.name == current_name)
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::media::audio::Port;
    use libpulse_binding::def::PortAvailable;

    fn device(name: &str, description: &str, nick: &str) -> Device {
        Device {
            name: name.to_owned(),
            description: description.to_owned(),
            nick: nick.to_owned(),
            ports: Vec::new(),
            active_port: String::new(),
        }
    }

    fn device_with_ports(name: &str, ports: &[&str], active_port: &str) -> Device {
        Device {
            ports: ports
                .iter()
                .map(|port| Port {
                    name: (*port).to_owned(),
                    description: port.to_uppercase(),
                    availability: PortAvailable::Yes,
                })
                .collect(),
            active_port: active_port.to_owned(),
            ..device(name, "Card", "")
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
    fn a_device_with_several_ports_is_listed_once_per_port() {
        let devices = [
            device_with_ports("a", &["speakers", "headphones"], "headphones"),
            device_with_ports("b", &["line"], "line"),
            device("c", "", ""),
        ];
        let choices = list_choices(&devices);
        let listed: Vec<(&str, Option<&str>, &str, bool)> = choices
            .iter()
            .map(|choice| {
                (
                    choice.name.as_str(),
                    choice.port.as_deref(),
                    choice.label.as_str(),
                    choice.is_port_active,
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                ("a", Some("speakers"), "Card · SPEAKERS", false),
                ("a", Some("headphones"), "Card · HEADPHONES", true),
                ("b", None, "Card", true),
                ("c", None, "c", true),
            ]
        );
    }

    #[test]
    fn the_current_choice_is_the_active_port_of_the_device_named_by_the_server_else_the_first() {
        let devices = [
            device("a", "", ""),
            device_with_ports("b", &["speakers", "headphones"], "headphones"),
            device("c", "", ""),
        ];
        let choices = list_choices(&devices);
        assert_eq!(resolve_current_index(&choices, "b"), 2);
        assert_eq!(resolve_current_index(&choices, "c"), 3);
        let choices_without_active_port = list_choices(&[
            device("a", "", ""),
            device_with_ports("b", &["speakers", "headphones"], "line"),
        ]);
        assert_eq!(resolve_current_index(&choices_without_active_port, "b"), 1);
        assert_eq!(resolve_current_index(&choices, "missing"), 0);
        assert_eq!(resolve_current_index(&[], "a"), 0);
    }
}
