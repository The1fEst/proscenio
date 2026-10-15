use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

use crate::core::process;

const CLOCK_TICKS: i32 = 2;
const NVIDIA_PMON_COMMAND: [&str; 6] = ["nvidia-smi", "pmon", "-c", "1", "-s", "um"];

unsafe extern "C" {
    fn sysconf(name: i32) -> i64;
}

#[derive(Clone, Copy, Default)]
pub struct Usage {
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub gpu_percent: Option<f64>,
    pub video_bytes: Option<u64>,
}

#[derive(Default, PartialEq, Debug)]
struct Drm {
    engines: HashMap<String, u64>,
    engine_cycles: HashMap<CycleKey, CycleCount>,
    video_bytes: Option<u64>,
}

#[derive(PartialEq, Eq, Hash, Debug)]
struct CycleKey {
    gpu_engine: String,
    client: String,
}

#[derive(PartialEq, Debug)]
struct CycleCount {
    busy_cycles: u64,
    total_cycles: u64,
    capacity: u64,
}

#[derive(Default)]
struct PartialCycleCount {
    busy_cycles: Option<u64>,
    total_cycles: Option<u64>,
    capacity: u64,
}

#[derive(Default)]
pub struct Meter {
    last_cpu_sample: RefCell<Option<(u64, Instant)>>,
    last_engine_sample: RefCell<Option<(HashMap<String, u64>, Instant)>>,
    last_cycle_sample: RefCell<Option<HashMap<CycleKey, CycleCount>>>,
}

impl Meter {
    pub fn new() -> Rc<Self> {
        Rc::new(Meter::default())
    }

    pub fn sample(self: &Rc<Self>, on_usage: impl Fn(Usage) + 'static) {
        let now = Instant::now();
        let mut usage = Usage {
            cpu_percent: self.sample_cpu_percent(now),
            memory_bytes: std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|status| parse_resident_bytes(&status)),
            ..Usage::default()
        };
        if let Some((gpu_percent, video_bytes)) = self.update_drm_usage(read_drm(), now) {
            usage.gpu_percent = gpu_percent;
            usage.video_bytes = video_bytes;
            on_usage(usage);
            return;
        }
        if !process::exists("nvidia-smi") {
            on_usage(usage);
            return;
        }
        process::read(&NVIDIA_PMON_COMMAND, move |output| {
            let (gpu_percent, video_bytes) = parse_pmon_usage(&output, std::process::id());
            on_usage(Usage {
                gpu_percent,
                video_bytes,
                ..usage
            });
        });
    }

    fn update_drm_usage(
        &self,
        drm: Option<Drm>,
        now: Instant,
    ) -> Option<(Option<f64>, Option<u64>)> {
        let Some(drm) = drm else {
            self.last_engine_sample.replace(None);
            self.last_cycle_sample.replace(None);
            return None;
        };
        let time_percent = if drm.engines.is_empty() {
            self.last_engine_sample.replace(None);
            None
        } else {
            self.update_busiest_engine_percent(drm.engines, now)
        };
        let cycle_percent = if drm.engine_cycles.is_empty() {
            self.last_cycle_sample.replace(None);
            None
        } else {
            self.update_busiest_cycle_percent(drm.engine_cycles)
        };
        let gpu_percent = time_percent
            .into_iter()
            .chain(cycle_percent)
            .reduce(f64::max);
        Some((gpu_percent, drm.video_bytes))
    }

    fn update_busiest_cycle_percent(
        &self,
        engine_cycles: HashMap<CycleKey, CycleCount>,
    ) -> Option<f64> {
        let previous_sample = self.last_cycle_sample.take();
        let busiest_percent = previous_sample.map(|previous_cycles| {
            let mut engine_percents: HashMap<&str, f64> = HashMap::new();
            for (key, cycles) in &engine_cycles {
                let percent = engine_percents.entry(key.gpu_engine.as_str()).or_default();
                let Some(previous) = previous_cycles.get(key) else {
                    continue;
                };
                let Some(total) = cycles
                    .total_cycles
                    .checked_sub(previous.total_cycles)
                    .filter(|total| *total > 0)
                else {
                    continue;
                };
                let busy_cycle_delta = cycles.busy_cycles.saturating_sub(previous.busy_cycles);
                let capacity_total = total.saturating_mul(cycles.capacity.max(1));
                *percent += busy_cycle_delta as f64 / capacity_total as f64 * 100.0;
            }
            engine_percents
                .into_values()
                .reduce(f64::max)
                .map(|percent| percent.min(100.0))
        });
        self.last_cycle_sample.replace(Some(engine_cycles));
        busiest_percent.flatten()
    }

    fn sample_cpu_percent(&self, now: Instant) -> Option<f64> {
        let ticks = cpu_ticks(&std::fs::read_to_string("/proc/self/stat").ok()?)?;
        self.update_cpu_percent(ticks, now)
    }

    fn update_cpu_percent(&self, ticks: u64, now: Instant) -> Option<f64> {
        let per_second = unsafe { sysconf(CLOCK_TICKS) }.max(1) as f64;
        let previous_sample = self.last_cpu_sample.replace(Some((ticks, now)));
        let (previous_ticks, previous_time) = previous_sample?;
        let seconds = now.duration_since(previous_time).as_secs_f64();
        (seconds > 0.0)
            .then(|| ticks.saturating_sub(previous_ticks) as f64 / per_second / seconds * 100.0)
    }

    fn update_busiest_engine_percent(
        &self,
        engines: HashMap<String, u64>,
        now: Instant,
    ) -> Option<f64> {
        let previous_sample = self
            .last_engine_sample
            .replace(Some((engines.clone(), now)));
        let (previous_engines, previous_time) = previous_sample?;
        let elapsed = now.duration_since(previous_time).as_nanos() as f64;
        if elapsed <= 0.0 {
            return None;
        }
        engines
            .iter()
            .map(|(engine, busy)| {
                let previous_busy = previous_engines.get(engine).copied().unwrap_or(*busy);
                busy.saturating_sub(previous_busy) as f64 / elapsed * 100.0
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

fn parse_resident_bytes(status: &str) -> Option<u64> {
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
    let mut shared_bytes: Option<u64> = None;
    let mut cycle_counts: HashMap<CycleKey, PartialCycleCount> = HashMap::new();
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
        let device = field("drm-pdev:").unwrap_or_default();
        if !clients.insert(format!("{driver}/{device}/{client}")) {
            continue;
        }
        let capacities: HashMap<&str, u64> = text
            .lines()
            .filter_map(|line| line.strip_prefix("drm-engine-capacity-")?.split_once(':'))
            .filter_map(|(engine, value)| Some((engine, parse_counter(value)?.max(1))))
            .collect();
        let capacity = |engine: &str| capacities.get(engine).copied().unwrap_or(1);
        let mut total_regions: HashMap<&str, u64> = HashMap::new();
        let mut legacy_regions: HashMap<&str, u64> = HashMap::new();
        let cycle_key = |engine: &str| CycleKey {
            gpu_engine: format!("{driver}/{device}/{engine}"),
            client: client.to_owned(),
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            if let Some(engine) = key.strip_prefix("drm-total-cycles-") {
                let Some(count) = parse_counter(value) else {
                    continue;
                };
                cycle_counts
                    .entry(cycle_key(engine))
                    .or_default()
                    .total_cycles = Some(count);
            } else if let Some(engine) = key.strip_prefix("drm-cycles-") {
                let Some(count) = parse_counter(value) else {
                    continue;
                };
                let cycles = cycle_counts.entry(cycle_key(engine)).or_default();
                cycles.busy_cycles = Some(count);
                cycles.capacity = capacity(engine);
            } else if let Some(engine) = key.strip_prefix("drm-engine-") {
                if engine.starts_with("capacity-") {
                    continue;
                }
                let Some(busy) = parse_counter(value) else {
                    continue;
                };
                let total_busy = drm
                    .engines
                    .entry(format!("{driver}/{device}/{engine}"))
                    .or_default();
                *total_busy = total_busy.saturating_add(busy / capacity(engine));
            } else if let Some(region) = key.strip_prefix("drm-total-") {
                if let Some(bytes) = parse_size(value) {
                    let region_bytes = total_regions.entry(region).or_default();
                    *region_bytes = region_bytes.saturating_add(bytes);
                }
            } else if let Some(region) = key.strip_prefix("drm-memory-") {
                if let Some(bytes) = parse_size(value) {
                    let region_bytes = legacy_regions.entry(region).or_default();
                    *region_bytes = region_bytes.saturating_add(bytes);
                }
            }
        }
        legacy_regions.retain(|region, _| !total_regions.contains_key(region));
        for (region, bytes) in total_regions.into_iter().chain(legacy_regions) {
            if region.contains("vram") || region.contains("local") {
                drm.video_bytes = Some(drm.video_bytes.unwrap_or(0).saturating_add(bytes));
            } else {
                shared_bytes = Some(shared_bytes.unwrap_or(0).saturating_add(bytes));
            }
        }
    }
    if drm.video_bytes.is_none_or(|bytes| bytes == 0) {
        drm.video_bytes = shared_bytes.or(drm.video_bytes);
    }
    drm.engine_cycles = cycle_counts
        .into_iter()
        .filter_map(|(key, partial)| {
            Some((
                key,
                CycleCount {
                    busy_cycles: partial.busy_cycles?,
                    total_cycles: partial.total_cycles?,
                    capacity: partial.capacity,
                },
            ))
        })
        .collect();
    let has_reading =
        !drm.engines.is_empty() || !drm.engine_cycles.is_empty() || drm.video_bytes.is_some();
    has_reading.then_some(drm)
}

fn parse_counter(value: &str) -> Option<u64> {
    value.split_whitespace().next()?.parse().ok()
}

fn parse_size(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number: u64 = parts.next()?.parse().ok()?;
    let unit = match parts.next() {
        None => 1,
        Some("KiB") => 1 << 10,
        Some("MiB") => 1 << 20,
        Some("GiB") => 1 << 30,
        Some(_) => return None,
    };
    Some(number.saturating_mul(unit))
}

fn parse_pmon_usage(output: &str, pid: u32) -> (Option<f64>, Option<u64>) {
    let Some((columns, pid_index)) = output
        .lines()
        .filter_map(|line| line.strip_prefix('#'))
        .find_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            let pid_index = columns.iter().position(|column| *column == "pid")?;
            Some((columns, pid_index))
        })
    else {
        return (None, None);
    };
    let column_index = |name: &str| columns.iter().position(|column| *column == name);
    let (sm_index, fb_index) = (column_index("sm"), column_index("fb"));
    let device_indices: Vec<usize> = ["gpu", "gi", "ci"]
        .into_iter()
        .filter_map(column_index)
        .collect();
    let pid = pid.to_string();
    let rows: Vec<Vec<&str>> = output
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .filter(|fields| fields.get(pid_index) == Some(&pid.as_str()))
        .collect();
    let gpu_percent = rows
        .iter()
        .filter_map(|fields| fields.get(sm_index?))
        .map(|value| value.parse::<f64>().unwrap_or(0.0))
        .reduce(f64::max);
    let mut video_mib_by_device: HashMap<Vec<Option<&str>>, u64> = HashMap::new();
    for fields in &rows {
        let Some(mib) = fb_index
            .and_then(|index| fields.get(index))
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        let device = device_indices
            .iter()
            .map(|index| fields.get(*index).copied())
            .collect();
        let device_mib = video_mib_by_device.entry(device).or_default();
        *device_mib = (*device_mib).max(mib);
    }
    let video_bytes = video_mib_by_device
        .into_values()
        .map(|mib| mib.saturating_mul(1 << 20))
        .reduce(u64::saturating_add);
    (gpu_percent, video_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn drm_fdinfo_sums_each_client_once_and_prefers_video_memory() {
        let amd_fdinfo = "pos:\t0\nflags:\t02100002\ndrm-driver:\tamdgpu\ndrm-client-id:\t12\ndrm-engine-gfx:\t5000 ns\ndrm-engine-compute:\t100 ns\ndrm-memory-vram:\t20480 KiB\ndrm-memory-gtt:\t4096 KiB\n".to_owned();
        let intel_fdinfo = "drm-driver:\ti915\ndrm-pdev:\t0000:00:02.0\ndrm-client-id:\t3\ndrm-engine-render:\t700 ns\ndrm-engine-capacity-render:\t2\ndrm-total-system:\t8 MiB\n".to_owned();
        let usage = drm_usage(&[amd_fdinfo.clone(), amd_fdinfo, "pos:\t0\n".to_owned()]).unwrap();
        assert_eq!(usage.engines["amdgpu//gfx"], 5000);
        assert_eq!(usage.video_bytes, Some(20 << 20));
        let intel_usage = drm_usage(&[intel_fdinfo]).unwrap();
        assert_eq!(intel_usage.video_bytes, Some(8 << 20));
        assert_eq!(intel_usage.engines["i915/0000:00:02.0/render"], 350);
        assert_eq!(drm_usage(&["pos:\t0\n".to_owned()]), None);
    }

    #[test]
    fn a_memory_region_in_both_key_styles_counts_once() {
        let amd_fdinfo = "drm-driver:\tamdgpu\ndrm-client-id:\t12\ndrm-memory-vram:\t20480 KiB\n\
                   drm-total-vram:\t24 MiB\ndrm-memory-gtt:\t4096 KiB\n"
            .to_owned();
        assert_eq!(
            drm_usage(&[amd_fdinfo]).unwrap().video_bytes,
            Some(24 << 20)
        );
        let gtt_in_both_keys = "drm-driver:\tamdgpu\ndrm-client-id:\t13\ndrm-memory-gtt:\t4 MiB\n\
                              drm-total-gtt:\t4 MiB\n"
            .to_owned();
        assert_eq!(
            drm_usage(&[gtt_in_both_keys]).unwrap().video_bytes,
            Some(4 << 20)
        );
        let unreadable_total_fdinfo =
            "drm-driver:\tamdgpu\ndrm-client-id:\t14\ndrm-total-vram:\t5 XB\n\
                                drm-memory-vram:\t20480 KiB\n"
                .to_owned();
        assert_eq!(
            drm_usage(&[unreadable_total_fdinfo]).unwrap().video_bytes,
            Some(20 << 20)
        );
    }

    #[test]
    fn drm_fdinfo_video_memory_is_known_only_from_a_readable_size() {
        let cases = [
            ("drm-engine-gr:\t900 ns\ndrm-memory-vram:\tN/A\n", None),
            ("drm-engine-gr:\t900 ns\ndrm-memory-vram:\t2 TiB\n", None),
            ("drm-memory-vram:\t0 KiB\n", Some(0)),
            (
                "drm-memory-vram:\t18446744073709551615 KiB\ndrm-memory-local0:\t1 KiB\n",
                Some(u64::MAX),
            ),
            (
                "drm-memory-vram:\t0 KiB\ndrm-memory-gtt:\t64 MiB\n",
                Some(64 << 20),
            ),
            ("drm-memory-gtt:\t64 MiB\n", Some(64 << 20)),
            ("drm-total-system0:\t4096\n", Some(4096)),
            (
                "drm-total-local0:\t64 MiB\ndrm-total-stolen-local0:\t8 MiB\n",
                Some(72 << 20),
            ),
        ];
        for (memory, video_bytes) in cases {
            let fdinfo = format!("drm-driver:\tamdgpu\ndrm-client-id:\t7\n{memory}");
            assert_eq!(
                drm_usage(&[fdinfo]).unwrap().video_bytes,
                video_bytes,
                "{memory}"
            );
        }
    }

    #[test]
    fn drm_fdinfo_without_readable_engines_or_memory_gives_no_reading() {
        let cases = [
            "drm-driver:\tnvidia-drm\ndrm-client-id:\t9\n",
            "drm-driver:\tnouveau\ndrm-client-id:\t5\ndrm-engine-gr:\tN/A\n",
            "drm-driver:\ti915\ndrm-client-id:\t3\ndrm-engine-capacity-video:\t2\n",
            "drm-driver:\tamdgpu\ndrm-client-id:\t4\ndrm-memory-vram:\tN/A\n",
        ];
        for fdinfo in cases {
            assert_eq!(drm_usage(&[fdinfo.to_owned()]), None, "{fdinfo}");
        }
    }

    #[test]
    fn nvidia_pmon_gives_the_shells_busiest_gpu_and_total_video_memory() {
        let output = "# gpu         pid   type     sm    mem    enc    dec    jpg    ofa     fb   ccpm    command \n# Idx           #    C/G      %      %      %      %      %      %     MB     MB    name \n    0        933     G      4      -      -      -      -      -    243      0    Hyprland       \n    0    3082096   C+G      -      -      -      -      -      -     16      0    proscenio      \n    0       4242     G      2      -      -      -      -      -      -      -    kitty          \n";
        assert_eq!(
            parse_pmon_usage(output, 3082096),
            (Some(0.0), Some(16 << 20))
        );
        assert_eq!(parse_pmon_usage(output, 933).0, Some(4.0));
        assert_eq!(parse_pmon_usage(output, 4242), (Some(2.0), None));
        assert_eq!(parse_pmon_usage(output, 1), (None, None));
        let two_gpus = "# gpu        pid  type    sm   mem   enc   dec    fb   command\n    0       2105     G     7     1     -     -    96   proscenio\n    1       2105     G    30     2     -     -    40   proscenio\n    1       2106     G    90     2     -     -   500   other\n    1       2105     C     -     -     -     -    40   proscenio\n    2       2105     G     -     -     -     -     -   proscenio\n";
        assert_eq!(
            parse_pmon_usage(two_gpus, 2105),
            (Some(30.0), Some(136 << 20))
        );
        let instances = "# gpu  gi  ci        pid  type    sm    fb   command\n    0   1   0       2105     G     5   200   proscenio\n    0   2   0       2105     G     6   100   proscenio\n";
        assert_eq!(
            parse_pmon_usage(instances, 2105),
            (Some(6.0), Some(300 << 20))
        );
        let without_devices = "# pid  type    sm    fb   command\n   2105     G     5    96   proscenio\n   2105     G     6    40   proscenio\n";
        assert_eq!(
            parse_pmon_usage(without_devices, 2105),
            (Some(6.0), Some(96 << 20))
        );
        let older_output = "# gpu        pid  type    sm   mem   enc   dec    fb   command\n# Idx          #   C/G     %     %     %     %    MB   name\n    0       2105     G     7     1     -     -    96   proscenio\n";
        assert_eq!(
            parse_pmon_usage(older_output, 2105),
            (Some(7.0), Some(96 << 20))
        );
        assert_eq!(
            parse_pmon_usage("    0       2105     G     7    96   proscenio\n", 2105),
            (None, None)
        );
    }

    #[test]
    fn cpu_ticks_that_go_backwards_read_as_idle() {
        let meter = Meter::default();
        let start = Instant::now();
        let second = |count| start + Duration::from_secs(count);
        let per_second = unsafe { sysconf(CLOCK_TICKS) }.max(1) as f64;
        assert_eq!(meter.update_cpu_percent(300, start), None);
        assert_eq!(meter.update_cpu_percent(290, second(1)), Some(0.0));
        assert_eq!(
            meter.update_cpu_percent(390, second(2)),
            Some(100.0 / per_second * 100.0)
        );
    }

    #[test]
    fn a_gap_without_drm_readings_restarts_the_gpu_measurement() {
        let start = Instant::now();
        let after_seconds = |count| start + Duration::from_secs(count);
        let drm_sample = |busy_nanos: u64| Drm {
            engines: HashMap::from([("amdgpu/gfx".to_owned(), busy_nanos)]),
            ..Drm::default()
        };
        let memory_only = Drm {
            video_bytes: Some(1 << 20),
            ..Drm::default()
        };
        let gaps = [None, Some(memory_only)];
        for gap in gaps {
            let meter = Meter::default();
            let gap_usage = gap.as_ref().map(|drm| (None, drm.video_bytes));
            assert_eq!(
                meter.update_drm_usage(Some(drm_sample(0)), start),
                Some((None, None))
            );
            assert_eq!(meter.update_drm_usage(gap, after_seconds(1)), gap_usage);
            assert_eq!(
                meter.update_drm_usage(Some(drm_sample(10_000_000_000)), after_seconds(10)),
                Some((None, None))
            );
            assert_eq!(
                meter.update_drm_usage(Some(drm_sample(11_000_000_000)), after_seconds(11)),
                Some((Some(100.0), None))
            );
        }
    }

    #[test]
    fn xe_engine_cycles_give_the_gpu_load_and_never_count_as_memory() {
        let fdinfo = |client: u32, busy_cycles: u64, total_cycles: u64| {
            format!(
                "drm-driver:\txe\ndrm-pdev:\t0000:00:02.0\ndrm-client-id:\t{client}\n\
                 drm-cycles-rcs:\t{busy_cycles}\ndrm-total-cycles-rcs:\t{total_cycles}\n\
                 drm-total-system0:\t2 MiB\n"
            )
        };
        let start = Instant::now();
        let after_seconds = |count| start + Duration::from_secs(count);
        let meter = Meter::default();
        assert_eq!(
            meter.update_drm_usage(drm_usage(&[fdinfo(4, 100, 1000)]), start),
            Some((None, Some(2 << 20)))
        );
        assert_eq!(
            meter.update_drm_usage(drm_usage(&[fdinfo(4, 400, 2000)]), after_seconds(2)),
            Some((Some(30.0), Some(2 << 20)))
        );
        let two_clients = [fdinfo(4, 600, 3000), fdinfo(5, 50, 3000)];
        assert_eq!(
            meter.update_drm_usage(drm_usage(&two_clients), after_seconds(4)),
            Some((Some(20.0), Some(4 << 20)))
        );
        let fdinfos_after_close = [fdinfo(5, 450, 4000)];
        assert_eq!(
            meter.update_drm_usage(drm_usage(&fdinfos_after_close), after_seconds(6)),
            Some((Some(40.0), Some(2 << 20)))
        );
        let fdinfos_after_reset = [fdinfo(5, 460, 10)];
        assert_eq!(
            meter.update_drm_usage(drm_usage(&fdinfos_after_reset), after_seconds(8)),
            Some((Some(0.0), Some(2 << 20)))
        );
        let video_engines = |busy_cycles: u64, total_cycles: u64| {
            format!(
                "drm-driver:\txe\ndrm-client-id:\t7\ndrm-engine-capacity-vcs:\t2\n\
                 drm-cycles-vcs:\t{busy_cycles}\ndrm-total-cycles-vcs:\t{total_cycles}\n"
            )
        };
        let video_meter = Meter::default();
        video_meter.update_drm_usage(drm_usage(&[video_engines(0, 0)]), start);
        assert_eq!(
            video_meter.update_drm_usage(drm_usage(&[video_engines(1000, 1000)]), after_seconds(2)),
            Some((Some(50.0), None))
        );
        let busy_only_fdinfo =
            "drm-driver:\tpanfrost\ndrm-client-id:\t6\ndrm-cycles-fragment:\t9\n";
        assert_eq!(drm_usage(&[busy_only_fdinfo.to_owned()]), None);
    }

    #[test]
    fn process_counters_come_from_proc_files() {
        let stat = "1234 (pro scenio) S 1 2 3 4 5 6 7 8 9 10 250 50 0 0 20 0 5";
        assert_eq!(cpu_ticks(stat), Some(300));
        assert_eq!(
            parse_resident_bytes("Name:\tproscenio\nVmRSS:\t  312000 kB\n"),
            Some(312000 * 1024)
        );
    }
}
