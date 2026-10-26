use std::collections::{BTreeSet, HashMap};
use std::path::Path;

const BROKER_EXECUTABLES: [&str; 2] = ["pipewire", "wireplumber"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraUse {
    pub executable: String,
    pub device: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    pub uses: Vec<CameraUse>,
    pub brokered_devices: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Candidate {
    pid: u32,
    executable: String,
    start_ticks: u64,
    device_number: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct Device {
    name: String,
    is_loopback: bool,
    has_producer: bool,
}

pub fn scan(proc_root: &Path, sys_root: &Path) -> Scan {
    let has_devices = std::fs::read_dir(sys_root).is_ok_and(|mut entries| entries.next().is_some());
    if !has_devices {
        return Scan::default();
    }
    let holders = read_holders(proc_root);
    let devices: HashMap<u32, Device> = holders
        .iter()
        .map(|(_, device)| *device)
        .collect::<BTreeSet<u32>>()
        .into_iter()
        .map(|number| (number, read_device(sys_root, number)))
        .collect();
    let candidates = holders
        .into_iter()
        .map(|(pid, device_number)| Candidate {
            pid,
            executable: read_executable(proc_root, pid),
            start_ticks: read_start_ticks(proc_root, pid),
            device_number,
        })
        .filter(|candidate| !candidate.executable.is_empty())
        .collect();
    select_uses(candidates, &devices)
}

fn select_uses(candidates: Vec<Candidate>, devices: &HashMap<u32, Device>) -> Scan {
    let mut scan = Scan::default();
    let mut numbers: Vec<u32> = candidates
        .iter()
        .map(|candidate| candidate.device_number)
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    for number in numbers {
        let Some(device) = devices.get(&number) else {
            continue;
        };
        let (brokers, mut holders): (Vec<&Candidate>, Vec<&Candidate>) = candidates
            .iter()
            .filter(|candidate| candidate.device_number == number)
            .partition(|candidate| BROKER_EXECUTABLES.contains(&candidate.executable.as_str()));
        if !brokers.is_empty() {
            scan.brokered_devices.push(device.name.clone());
        }
        holders.sort_by_key(|candidate| (candidate.start_ticks, candidate.pid));
        if device.is_loopback {
            if !device.has_producer {
                continue;
            }
            if !holders.is_empty() {
                holders.remove(0);
            }
        }
        scan.uses
            .extend(holders.into_iter().map(|candidate| CameraUse {
                executable: candidate.executable.clone(),
                device: device.name.clone(),
            }));
    }
    scan
}

fn read_holders(proc_root: &Path) -> BTreeSet<(u32, u32)> {
    let mut holders = BTreeSet::new();
    let Ok(processes) = std::fs::read_dir(proc_root) else {
        return holders;
    };
    for process in processes.flatten() {
        let Some(pid) = process
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(descriptors) = std::fs::read_dir(process.path().join("fd")) else {
            continue;
        };
        for descriptor in descriptors.flatten() {
            let device = std::fs::read_link(descriptor.path())
                .ok()
                .and_then(|target| parse_video_device(&target.to_string_lossy()));
            if let Some(device) = device {
                holders.insert((pid, device));
            }
        }
    }
    holders
}

fn parse_video_device(target: &str) -> Option<u32> {
    target.strip_prefix("/dev/video")?.parse().ok()
}

fn read_device(sys_root: &Path, number: u32) -> Device {
    let directory = sys_root.join(format!("video{number}"));
    let name = std::fs::read_to_string(directory.join("name"))
        .map(|name| name.trim().to_owned())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("/dev/video{number}"));
    Device {
        name,
        is_loopback: directory.join("max_openers").exists(),
        has_producer: std::fs::read_to_string(directory.join("state"))
            .is_ok_and(|state| state.trim() == "capture"),
    }
}

fn read_executable(proc_root: &Path, pid: u32) -> String {
    let directory = proc_root.join(pid.to_string());
    std::fs::read_link(directory.join("exe"))
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| {
            std::fs::read_to_string(directory.join("comm"))
                .ok()
                .map(|name| name.trim().to_owned())
        })
        .unwrap_or_default()
}

fn read_start_ticks(proc_root: &Path, pid: u32) -> u64 {
    std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat"))
        .ok()
        .and_then(|stat| parse_start_ticks(&stat))
        .unwrap_or(u64::MAX)
}

fn parse_start_ticks(stat: &str) -> Option<u64> {
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(pid: u32, executable: &str, start_ticks: u64, device_number: u32) -> Candidate {
        Candidate {
            pid,
            executable: executable.to_owned(),
            start_ticks,
            device_number,
        }
    }

    fn device(name: &str, is_loopback: bool, has_producer: bool) -> Device {
        Device {
            name: name.to_owned(),
            is_loopback,
            has_producer,
        }
    }

    fn camera_use(executable: &str, device: &str) -> CameraUse {
        CameraUse {
            executable: executable.to_owned(),
            device: device.to_owned(),
        }
    }

    #[test]
    fn a_video_device_path_names_its_number() {
        assert_eq!(parse_video_device("/dev/video0"), Some(0));
        assert_eq!(parse_video_device("/dev/video10"), Some(10));
        assert_eq!(parse_video_device("/dev/video10 (deleted)"), None);
        assert_eq!(parse_video_device("/dev/media0"), None);
        assert_eq!(parse_video_device("/dev/videofoo"), None);
        assert_eq!(parse_video_device("socket:[123]"), None);
    }

    #[test]
    fn the_start_time_is_the_twenty_second_field_after_a_comm_with_spaces_and_parentheses() {
        let stat = "42 (my (odd) cam) S 1 42 42 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 5 0 987654 1000 10 18446744073709551615";
        assert_eq!(parse_start_ticks(stat), Some(987654));
        assert_eq!(parse_start_ticks("42 (short) S 1"), None);
    }

    #[test]
    fn a_real_camera_is_in_use_by_every_process_holding_it() {
        let devices = HashMap::from([(0, device("Integrated Camera", false, false))]);
        let scan = select_uses(
            vec![
                candidate(10, "brave", 200, 0),
                candidate(11, "zoom", 100, 0),
            ],
            &devices,
        );
        assert_eq!(
            scan.uses,
            [
                camera_use("zoom", "Integrated Camera"),
                camera_use("brave", "Integrated Camera")
            ]
        );
        assert!(scan.brokered_devices.is_empty());
    }

    #[test]
    fn a_virtual_camera_is_in_use_by_the_holders_that_started_after_its_producer() {
        let devices = HashMap::from([(10, device("Phone Camera", true, true))]);
        let cases = [
            (vec![candidate(5, "ffmpeg", 100, 10)], vec![]),
            (
                vec![candidate(9, "brave", 300, 10), candidate(5, "obs", 100, 10)],
                vec![camera_use("brave", "Phone Camera")],
            ),
            (
                vec![
                    candidate(9, "brave", 300, 10),
                    candidate(5, "obs", 100, 10),
                    candidate(7, "zoom", 200, 10),
                ],
                vec![
                    camera_use("zoom", "Phone Camera"),
                    camera_use("brave", "Phone Camera"),
                ],
            ),
        ];
        for (candidates, expected) in cases {
            assert_eq!(select_uses(candidates, &devices).uses, expected);
        }
    }

    #[test]
    fn a_virtual_camera_without_a_producer_is_not_in_use() {
        let devices = HashMap::from([(10, device("Phone Camera", true, false))]);
        let scan = select_uses(
            vec![
                candidate(5, "brave", 100, 10),
                candidate(6, "zoom", 200, 10),
            ],
            &devices,
        );
        assert_eq!(scan, Scan::default());
    }

    #[test]
    fn a_sound_server_holding_a_camera_is_reported_apart_from_the_apps() {
        let devices = HashMap::from([
            (0, device("Integrated Camera", false, false)),
            (10, device("Phone Camera", true, true)),
        ]);
        let scan = select_uses(
            vec![
                candidate(1, "wireplumber", 10, 0),
                candidate(2, "pipewire", 11, 10),
                candidate(5, "obs", 100, 10),
            ],
            &devices,
        );
        assert_eq!(scan.uses, []);
        assert_eq!(scan.brokered_devices, ["Integrated Camera", "Phone Camera"]);
    }

    #[test]
    fn the_scan_reads_open_video_devices_from_proc_and_sys() {
        let root = std::env::temp_dir().join(format!("proscenio-cameras-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (proc_root, sys_root) = (root.join("proc"), root.join("sys"));
        let write = |path: std::path::PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        let link = |from: &str, to: std::path::PathBuf| {
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(from, to).unwrap();
        };
        write(sys_root.join("video10/name"), "Phone Camera\n");
        write(sys_root.join("video10/max_openers"), "10\n");
        write(sys_root.join("video10/state"), "capture\n");
        link("/dev/video10", proc_root.join("100/fd/5"));
        link("/usr/bin/obs", proc_root.join("100/exe"));
        write(
            proc_root.join("100/stat"),
            "100 (obs) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 500 0 0",
        );
        link("/dev/video10", proc_root.join("200/fd/7"));
        link("/dev/null", proc_root.join("200/fd/8"));
        link("/usr/lib/brave/brave", proc_root.join("200/exe"));
        write(
            proc_root.join("200/stat"),
            "200 (brave) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 900 0 0",
        );
        link("/dev/video10", proc_root.join("400/fd/3"));
        write(proc_root.join("300/stat"), "300 (idle) S 1");
        write(proc_root.join("not-a-pid/fd/1"), "");
        assert_eq!(
            scan(&proc_root, &sys_root).uses,
            [camera_use("brave", "Phone Camera")]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
