use serde_json::Value;

use crate::platform::hypr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Keyboard,
    Mouse,
    Tablet,
    Touch,
    Switch,
}

const KINDS: [(&str, Kind); 5] = [
    ("keyboards", Kind::Keyboard),
    ("mice", Kind::Mouse),
    ("tablets", Kind::Tablet),
    ("touch", Kind::Touch),
    ("switches", Kind::Switch),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub name: String,
    pub kind: Kind,
    pub main: bool,
    pub keymap: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KernelDevice {
    pub name: String,
    pub phys: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub label: String,
    pub devices: Vec<Device>,
}

pub fn parse_kernel(text: &str) -> Vec<KernelDevice> {
    text.split("\n\n")
        .filter_map(|block| {
            let field = |prefix: &str| {
                block
                    .lines()
                    .find_map(|line| line.strip_prefix(prefix))
                    .map(str::to_owned)
            };
            let name = field("N: Name=")?.trim_matches('"').to_owned();
            let phys = field("P: Phys=").unwrap_or_default();
            Some(KernelDevice { name, phys })
        })
        .collect()
}

pub fn parse_hyprland(devices: &Value) -> Vec<Device> {
    let mut found = Vec::new();
    for (key, kind) in KINDS {
        let Some(list) = devices.get(key).and_then(Value::as_array) else {
            continue;
        };
        for device in list {
            let Some(name) = device.get("name").and_then(Value::as_str) else {
                continue;
            };
            found.push(Device {
                name: name.to_owned(),
                kind,
                main: device.get("main").and_then(Value::as_bool) == Some(true),
                keymap: device
                    .get("active_keymap")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            });
        }
    }
    found
}

fn hyprland_name(kernel: &str) -> String {
    kernel.to_lowercase().replace(' ', "-")
}

fn kernel_of<'a>(name: &str, kernel: &'a [KernelDevice]) -> Option<&'a KernelDevice> {
    let exact = |wanted: &str| {
        kernel
            .iter()
            .find(|device| hyprland_name(&device.name) == wanted)
    };
    exact(name).or_else(|| {
        let (base, suffix) = name.rsplit_once('-')?;
        suffix.parse::<u32>().ok()?;
        exact(base)
    })
}

fn group_key(name: &str, kernel: Option<&KernelDevice>) -> String {
    match kernel {
        Some(device) if !device.phys.is_empty() => {
            let phys = device.phys.as_str();
            phys.rsplit_once("/input")
                .map_or(phys, |(root, _)| root)
                .to_owned()
        }
        Some(device) => format!(
            "virtual:{}",
            device
                .name
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_lowercase()
        ),
        None => format!("virtual:{}", name.split('-').next().unwrap_or_default()),
    }
}

fn label(names: &[String]) -> String {
    let words: Vec<Vec<&str>> = names
        .iter()
        .map(|name| name.split_whitespace().collect())
        .collect();
    let Some(first) = words.first() else {
        return String::new();
    };
    let shared = (0..first.len())
        .take_while(|&index| {
            words
                .iter()
                .all(|other| other.get(index) == first.get(index))
        })
        .count();
    let mut kept: Vec<&str> = if shared == 0 {
        first.clone()
    } else {
        first[..shared].to_vec()
    };
    kept.dedup_by(|next, previous| next.eq_ignore_ascii_case(previous));
    kept.join(" ")
}

pub fn group(devices: &[Device], kernel: &[KernelDevice]) -> Vec<Group> {
    let mut keyed: Vec<(String, Vec<String>, Vec<Device>)> = Vec::new();
    for device in devices {
        let found = kernel_of(&device.name, kernel);
        let key = group_key(&device.name, found);
        let kernel_name = found.map_or_else(|| device.name.clone(), |found| found.name.clone());
        match keyed.iter_mut().find(|(known, _, _)| *known == key) {
            Some((_, names, members)) => {
                names.push(kernel_name);
                members.push(device.clone());
            }
            None => keyed.push((key, vec![kernel_name], vec![device.clone()])),
        }
    }
    let mut groups: Vec<(bool, Group)> = keyed
        .into_iter()
        .map(|(key, names, mut members)| {
            members.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
            (
                key.starts_with("virtual:"),
                Group {
                    label: label(&names),
                    devices: members,
                },
            )
        })
        .collect();
    groups.sort_by(|(a_virtual, a), (b_virtual, b)| {
        a_virtual
            .cmp(b_virtual)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
    groups.into_iter().map(|(_, group)| group).collect()
}

pub fn read() -> Vec<Group> {
    let devices = hypr::json("devices")
        .map(|value| parse_hyprland(&value))
        .unwrap_or_default();
    let kernel = std::fs::read_to_string("/proc/bus/input/devices")
        .map(|text| parse_kernel(&text))
        .unwrap_or_default();
    group(&devices, &kernel)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROC: &str = "I: Bus=0003 Vendor=31e3 Product=1402 Version=0111\nN: Name=\"Wooting Wooting 80HE\"\nP: Phys=usb-0000:13:00.3-2.1/input1\nS: Sysfs=/devices/x/input/input8\n\nI: Bus=0003\nN: Name=\"Wooting Wooting 80HE Consumer Control\"\nP: Phys=usb-0000:13:00.3-2.1/input3\n\nN: Name=\"Wooting Wooting 80HE Mouse\"\nP: Phys=usb-0000:13:00.3-2.1/input3\n\nN: Name=\"LAMZU LAMZU Aurora 8K Receiver\"\nP: Phys=usb-0000:0f:00.0-3/input0\n\nN: Name=\"LAMZU LAMZU Aurora 8K Receiver Keyboard\"\nP: Phys=usb-0000:0f:00.0-3/input1\n\nN: Name=\"LAMZU LAMZU Aurora 8K Receiver\"\nP: Phys=usb-0000:0f:00.0-3/input1\n\nN: Name=\"libvirtualhid Keyboard\"\nP: Phys=\n\nN: Name=\"libvirtualhid Mouse (Absolute)\"\nP: Phys=\n";

    fn device(name: &str, kind: Kind) -> Device {
        Device {
            name: name.to_owned(),
            kind,
            main: false,
            keymap: String::new(),
        }
    }

    #[test]
    fn interfaces_of_one_piece_of_hardware_share_a_group() {
        let kernel = parse_kernel(PROC);
        assert_eq!(kernel.len(), 8);
        assert_eq!(kernel[0].phys, "usb-0000:13:00.3-2.1/input1");
        let devices = [
            device("wooting-wooting-80he", Kind::Keyboard),
            device("wooting-wooting-80he-mouse", Kind::Mouse),
            device("wooting-wooting-80he-consumer-control", Kind::Keyboard),
            device("lamzu-lamzu-aurora-8k-receiver-1", Kind::Mouse),
            device("lamzu-lamzu-aurora-8k-receiver", Kind::Keyboard),
            device("lamzu-lamzu-aurora-8k-receiver-keyboard", Kind::Keyboard),
            device("libvirtualhid-keyboard", Kind::Keyboard),
            device("libvirtualhid-mouse-(absolute)", Kind::Mouse),
            device("power-button-1", Kind::Keyboard),
        ];
        let groups = group(&devices, &kernel);
        let summary: Vec<(String, Vec<String>)> = groups
            .iter()
            .map(|group| {
                (
                    group.label.clone(),
                    group
                        .devices
                        .iter()
                        .map(|device| device.name.clone())
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "LAMZU Aurora 8K Receiver".to_owned(),
                    vec![
                        "lamzu-lamzu-aurora-8k-receiver".to_owned(),
                        "lamzu-lamzu-aurora-8k-receiver-keyboard".to_owned(),
                        "lamzu-lamzu-aurora-8k-receiver-1".to_owned(),
                    ]
                ),
                (
                    "Wooting 80HE".to_owned(),
                    vec![
                        "wooting-wooting-80he".to_owned(),
                        "wooting-wooting-80he-consumer-control".to_owned(),
                        "wooting-wooting-80he-mouse".to_owned(),
                    ]
                ),
                (
                    "libvirtualhid".to_owned(),
                    vec![
                        "libvirtualhid-keyboard".to_owned(),
                        "libvirtualhid-mouse-(absolute)".to_owned(),
                    ]
                ),
                (
                    "power-button-1".to_owned(),
                    vec!["power-button-1".to_owned()]
                ),
            ]
        );
    }
}
