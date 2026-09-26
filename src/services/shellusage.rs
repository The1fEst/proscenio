use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

use crate::core::process;

const CLOCK_TICKS: i32 = 2;
const NVIDIA_PROCESSES: [&str; 6] = ["nvidia-smi", "pmon", "-c", "1", "-s", "um"];

unsafe extern "C" {
    fn sysconf(name: i32) -> i64;
}

#[derive(Clone, Copy, Default)]
pub struct Usage {
    pub cpu: Option<f64>,
    pub memory: Option<u64>,
    pub gpu: Option<f64>,
    pub video: Option<u64>,
}

#[derive(Default, PartialEq, Debug)]
struct Drm {
    engines: HashMap<String, u64>,
    video: u64,
}

#[derive(Default)]
pub struct Meter {
    cpu: RefCell<Option<(u64, Instant)>>,
    engines: RefCell<Option<(HashMap<String, u64>, Instant)>>,
}

impl Meter {
    pub fn new() -> Rc<Self> {
        Rc::new(Meter::default())
    }

    pub fn sample(self: &Rc<Self>, done: impl Fn(Usage) + 'static) {
        let now = Instant::now();
        let mut usage = Usage {
            cpu: self.cpu(now),
            memory: std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|status| resident(&status)),
            ..Usage::default()
        };
        if let Some(drm) = read_drm() {
            usage.gpu = self.busiest_engine(drm.engines, now);
            usage.video = Some(drm.video);
            done(usage);
            return;
        }
        if !process::exists("nvidia-smi") {
            done(usage);
            return;
        }
        process::read(&NVIDIA_PROCESSES, move |output| {
            let (gpu, video) = pmon_row(&output, std::process::id());
            done(Usage {
                gpu,
                video,
                ..usage
            });
        });
    }

    fn cpu(&self, now: Instant) -> Option<f64> {
        let ticks = cpu_ticks(&std::fs::read_to_string("/proc/self/stat").ok()?)?;
        let per_second = unsafe { sysconf(CLOCK_TICKS) }.max(1) as f64;
        let previous = self.cpu.replace(Some((ticks, now)));
        let (before, then) = previous?;
        let seconds = now.duration_since(then).as_secs_f64();
        (seconds > 0.0).then(|| (ticks - before) as f64 / per_second / seconds * 100.0)
    }

    fn busiest_engine(&self, engines: HashMap<String, u64>, now: Instant) -> Option<f64> {
        let previous = self.engines.replace(Some((engines.clone(), now)));
        let (before, then) = previous?;
        let elapsed = now.duration_since(then).as_nanos() as f64;
        if elapsed <= 0.0 {
            return None;
        }
        engines
            .iter()
            .map(|(engine, busy)| {
                let earlier = before.get(engine).copied().unwrap_or(*busy);
                busy.saturating_sub(earlier) as f64 / elapsed * 100.0
            })
            .reduce(f64::max)
            .map(|percent| percent.min(100.0))
    }
}

fn cpu_ticks(stat: &str) -> Option<u64> {
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let user: u64 = fields.get(11)?.parse().ok()?;
    let system: u64 = fields.get(12)?.parse().ok()?;
    Some(user + system)
}

fn resident(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

fn read_drm() -> Option<Drm> {
    let entries = std::fs::read_dir("/proc/self/fdinfo").ok()?;
    let texts: Vec<String> = entries
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .collect();
    drm_usage(&texts)
}

fn drm_usage(fdinfos: &[String]) -> Option<Drm> {
    let mut clients = HashSet::new();
    let mut drm = Drm::default();
    let mut found = false;
    let mut shared = 0;
    for text in fdinfos {
        let field = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key))
                .map(str::trim)
        };
        let Some(client) = field("drm-client-id:") else {
            continue;
        };
        let driver = field("drm-driver:").unwrap_or_default();
        if !clients.insert(format!("{driver}/{client}")) {
            continue;
        }
        found = true;
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            if let Some(engine) = key.strip_prefix("drm-engine-") {
                let busy = value
                    .split_whitespace()
                    .next()
                    .and_then(|number| number.parse::<u64>().ok())
                    .unwrap_or(0);
                *drm.engines.entry(format!("{driver}/{engine}")).or_default() += busy;
            } else if let Some(region) = key
                .strip_prefix("drm-total-")
                .or_else(|| key.strip_prefix("drm-memory-"))
            {
                let bytes = size(value);
                if region.contains("vram") || region.starts_with("local") {
                    drm.video += bytes;
                } else {
                    shared += bytes;
                }
            }
        }
    }
    if drm.video == 0 {
        drm.video = shared;
    }
    found.then_some(drm)
}

fn size(value: &str) -> u64 {
    let mut parts = value.split_whitespace();
    let number: u64 = parts
        .next()
        .and_then(|number| number.parse().ok())
        .unwrap_or(0);
    let unit = match parts.next() {
        Some("KiB") => 1 << 10,
        Some("MiB") => 1 << 20,
        Some("GiB") => 1 << 30,
        _ => 1,
    };
    number * unit
}

fn pmon_row(output: &str, pid: u32) -> (Option<f64>, Option<u64>) {
    let pid = pid.to_string();
    let Some(fields) = output
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .find(|fields| fields.get(1) == Some(&pid.as_str()))
    else {
        return (None, None);
    };
    let number = |index: usize| {
        fields
            .get(index)
            .map(|value| value.parse::<f64>().unwrap_or(0.0))
    };
    let gpu = number(3);
    let video = number(9).map(|mib| (mib * 1024.0 * 1024.0) as u64);
    (gpu, video)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drm_fdinfo_sums_each_client_once_and_prefers_video_memory() {
        let amd = "pos:\t0\nflags:\t02100002\ndrm-driver:\tamdgpu\ndrm-client-id:\t12\ndrm-engine-gfx:\t5000 ns\ndrm-engine-compute:\t100 ns\ndrm-memory-vram:\t20480 KiB\ndrm-memory-gtt:\t4096 KiB\n".to_owned();
        let intel = "drm-driver:\ti915\ndrm-client-id:\t3\ndrm-engine-render:\t700 ns\ndrm-total-system:\t8 MiB\n".to_owned();
        let usage = drm_usage(&[amd.clone(), amd, "pos:\t0\n".to_owned()]).unwrap();
        assert_eq!(usage.engines["amdgpu/gfx"], 5000);
        assert_eq!(usage.video, 20 << 20);
        assert_eq!(drm_usage(&[intel]).unwrap().video, 8 << 20);
        assert_eq!(drm_usage(&["pos:\t0\n".to_owned()]), None);
    }

    #[test]
    fn nvidia_pmon_gives_the_shells_row() {
        let output = "# gpu         pid   type     sm    mem    enc    dec    jpg    ofa     fb   ccpm    command \n# Idx           #    C/G      %      %      %      %      %      %     MB     MB    name \n    0        933     G      4      -      -      -      -      -    243      0    Hyprland       \n    0    3082096   C+G      -      -      -      -      -      -     16      0    proscenio      \n";
        assert_eq!(pmon_row(output, 3082096), (Some(0.0), Some(16 << 20)));
        assert_eq!(pmon_row(output, 933).0, Some(4.0));
        assert_eq!(pmon_row(output, 1), (None, None));
    }

    #[test]
    fn process_counters_come_from_proc_files() {
        let stat = "1234 (pro scenio) S 1 2 3 4 5 6 7 8 9 10 250 50 0 0 20 0 5";
        assert_eq!(cpu_ticks(stat), Some(300));
        assert_eq!(
            resident("Name:\tproscenio\nVmRSS:\t  312000 kB\n"),
            Some(312000 * 1024)
        );
    }
}
