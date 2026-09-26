use std::cell::{Cell, RefCell};
use std::fs;

use crate::core::listeners::{Listeners, Subscription};

#[derive(Clone, Copy, Default)]
pub struct Usage {
    pub memory: f64,
    pub swap: f64,
    pub cpu: f64,
    pub memory_total: f64,
    pub memory_free: f64,
    pub swap_total: f64,
    pub swap_free: f64,
}

impl Usage {
    pub fn memory_used(&self) -> f64 {
        self.memory_total - self.memory_free
    }

    pub fn swap_used(&self) -> f64 {
        self.swap_total - self.swap_free
    }
}

#[derive(Default)]
pub struct Sampler {
    previous: Option<CpuTimes>,
    last_cpu: f64,
}

#[derive(Default)]
pub struct Resources {
    sampler: RefCell<Sampler>,
    usage: Cell<Usage>,
    listeners: Listeners,
}

impl Resources {
    pub fn usage(&self) -> Usage {
        self.usage.get()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn sample(&self) -> Result<(), String> {
        let usage = self
            .sampler
            .borrow_mut()
            .sample()
            .ok_or("/proc/meminfo cannot be read")?;
        self.usage.set(usage);
        self.listeners.notify();
        Ok(())
    }
}

impl Sampler {
    pub fn sample(&mut self) -> Option<Usage> {
        let mut usage = memory()?;
        usage.cpu = self.cpu();
        Some(usage)
    }

    fn cpu(&mut self) -> f64 {
        let Some(now) = cpu_times() else {
            return self.last_cpu;
        };
        let cpu = match self.previous {
            Some(before) if now.total > before.total => {
                (now.busy - before.busy) as f64 / (now.total - before.total) as f64
            }
            _ => self.last_cpu,
        };
        self.previous = Some(now);
        self.last_cpu = cpu;
        cpu
    }
}

#[derive(Clone, Copy)]
struct CpuTimes {
    busy: u64,
    total: u64,
}

fn cpu_times() -> Option<CpuTimes> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().next()?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(7)
        .filter_map(|field| field.parse().ok())
        .collect();
    if fields.len() < 7 {
        return None;
    }
    let total: u64 = fields.iter().sum();
    let idle = fields[3];
    Some(CpuTimes {
        busy: total - idle,
        total,
    })
}

fn memory() -> Option<Usage> {
    let info = fs::read_to_string("/proc/meminfo").ok()?;
    let mut total = 0u64;
    let mut available = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;
    for line in info.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(value) = rest.split_whitespace().next().and_then(|v| v.parse().ok()) else {
            continue;
        };
        match key {
            "MemTotal" => total = value,
            "MemAvailable" => available = value,
            "SwapTotal" => swap_total = value,
            "SwapFree" => swap_free = value,
            _ => {}
        }
    }
    Some(Usage {
        memory: fraction(total.saturating_sub(available), total),
        swap: fraction(swap_total.saturating_sub(swap_free), swap_total),
        cpu: 0.0,
        memory_total: total as f64,
        memory_free: available as f64,
        swap_total: swap_total as f64,
        swap_free: swap_free as f64,
    })
}

fn fraction(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OsRelease {
    pub name: String,
    pub home_url: String,
    pub documentation_url: String,
    pub support_url: String,
    pub bug_report_url: String,
    pub privacy_policy_url: String,
    pub logo: String,
}

pub fn os_release() -> OsRelease {
    parse_os_release(&fs::read_to_string("/etc/os-release").unwrap_or_default())
}

fn parse_os_release(text: &str) -> OsRelease {
    let raw = |key: &str| {
        let prefix = format!("{key}=");
        text.lines()
            .find_map(|line| line.strip_prefix(prefix.as_str()))
    };
    let quoted = |key: &str| {
        raw(key)
            .and_then(|value| value.strip_prefix('"'))
            .and_then(|value| value.split_once('"'))
            .map(|(value, _)| value.to_owned())
            .filter(|value| !value.is_empty())
    };
    let unquoted = |key: &str| {
        raw(key)
            .map(|value| value.trim_matches('"').to_owned())
            .filter(|value| !value.is_empty())
    };
    let name = quoted("PRETTY_NAME")
        .or_else(|| quoted("NAME").map(|name| without_linux(&name)))
        .unwrap_or_else(|| "Unknown".to_owned());
    let logo = unquoted("LOGO").unwrap_or_else(|| format!("{}-symbolic", distro_family(text)));
    OsRelease {
        name,
        home_url: quoted("HOME_URL").unwrap_or_default(),
        documentation_url: quoted("DOCUMENTATION_URL").unwrap_or_default(),
        support_url: quoted("SUPPORT_URL").unwrap_or_default(),
        bug_report_url: quoted("BUG_REPORT_URL").unwrap_or_default(),
        privacy_policy_url: quoted("PRIVACY_POLICY_URL").unwrap_or_default(),
        logo,
    }
}

fn without_linux(name: &str) -> String {
    match name.to_lowercase().find("linux") {
        Some(at) => format!("{}{}", &name[..at], &name[at + "linux".len()..])
            .trim()
            .to_owned(),
        None => name.trim().to_owned(),
    }
}

pub fn distro_family(release: &str) -> &'static str {
    if release.to_lowercase().contains("nyarch") {
        return "nyarch";
    }
    let id = release
        .lines()
        .find_map(|line| line.strip_prefix("ID="))
        .unwrap_or("")
        .trim_matches('"');
    match id {
        "artix" | "arch" => "arch",
        "manjaro" => "manjaro",
        "endeavouros" => "endeavouros",
        "cachyos" => "cachyos",
        "nixos" => "nixos",
        "fedora" => "fedora",
        "linuxmint" | "ubuntu" | "zorin" | "popos" => "ubuntu",
        "debian" | "raspbian" | "kali" => "debian",
        "funtoo" | "gentoo" => "gentoo",
        _ => "linux",
    }
}

#[derive(Clone, Default)]
pub struct Machine {
    pub hostname: String,
    pub kernel: String,
    pub processor: String,
    pub memory: String,
    pub session: String,
}

pub fn machine() -> Machine {
    let first_line = |path: &str| {
        fs::read_to_string(path)
            .map(|text| text.trim().to_owned())
            .unwrap_or_default()
    };
    let processor = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            let rest = line.strip_prefix("model name")?;
            let rest = rest.trim_start_matches([' ', '\t']);
            rest.strip_prefix(": ").map(str::to_owned)
        })
        .unwrap_or_default();
    let kilobytes = fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")?
                .split_whitespace()
                .next()?
                .parse::<f64>()
                .ok()
        })
        .unwrap_or(0.0);
    let memory = if kilobytes > 0.0 {
        format!("{:.1} GiB", kilobytes / 1024.0 / 1024.0)
    } else {
        String::new()
    };
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let wayland = std::env::var("WAYLAND_DISPLAY").is_ok_and(|display| !display.is_empty());
    let session = if desktop.trim().is_empty() {
        String::new()
    } else {
        let windowing = if wayland { "Wayland" } else { "X11" };
        format!("{} ({windowing})", desktop.trim())
    };
    Machine {
        hostname: first_line("/proc/sys/kernel/hostname"),
        kernel: first_line("/proc/sys/kernel/osrelease"),
        processor,
        memory,
        session,
    }
}

pub fn graphics(handler: impl FnOnce(String) + 'static) {
    crate::core::process::read(&["lspci", "-mm"], move |output| {
        handler(parse_graphics(&output))
    });
}

fn parse_graphics(lspci: &str) -> String {
    const DISPLAYS: [&str; 3] = [
        "VGA compatible controller",
        "3D controller",
        "Display controller",
    ];
    lspci
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('"').collect();
            let class = *fields.get(1)?;
            DISPLAYS.contains(&class).then(|| {
                let vendor = short_device_name(fields.get(3).copied().unwrap_or(""));
                let device = short_device_name(fields.get(5).copied().unwrap_or(""));
                format!("{vendor} {device}").trim().to_owned()
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn short_device_name(name: &str) -> String {
    if let Some((_, rest)) = name.split_once('[')
        && let Some((alias, _)) = rest.split_once(']')
        && !alias.is_empty()
    {
        return alias.to_owned();
    }
    const SUFFIXES: [&str; 12] = [
        "Technology Co., Ltd.",
        "Technology Co., Ltd",
        "Technology Co. Ltd.",
        "Technology Co. Ltd",
        "Technology Inc.",
        "Corporation",
        "Co., Ltd.",
        "Co., Ltd",
        "Co. Ltd.",
        "Co. Ltd",
        "Corp.",
        "Inc.",
    ];
    for suffix in SUFFIXES {
        if let Some(rest) = name
            .strip_suffix(suffix)
            .and_then(|rest| rest.strip_suffix(' '))
        {
            return rest.strip_suffix(',').unwrap_or(rest).trim().to_owned();
        }
    }
    name.trim().to_owned()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Disk {
    pub source: String,
    pub mount: String,
    pub fstype: String,
    pub used: u64,
    pub size: u64,
}

pub fn disks(handler: impl FnOnce(Vec<Disk>) + 'static) {
    crate::core::process::read(
        &[
            "df",
            "-B1",
            "--output=source,target,fstype,used,size",
            "-x",
            "squashfs",
        ],
        move |output| handler(parse_disks(&output)),
    );
}

fn parse_disks(df: &str) -> Vec<Disk> {
    let mut disks: Vec<Disk> = Vec::new();
    for line in df.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [source, mount, fstype, used, size] = fields[..] else {
            continue;
        };
        let Some(device) = source.strip_prefix("/dev/") else {
            continue;
        };
        let size: u64 = size.parse().unwrap_or(0);
        if size == 0 || disks.iter().any(|disk| disk.source == device) {
            continue;
        }
        disks.push(Disk {
            source: device.to_owned(),
            mount: mount.replace("\\040", " "),
            fstype: fstype.to_owned(),
            used: used.parse().unwrap_or(0),
            size,
        });
    }
    disks
}

pub fn human_size(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let (mut value, mut unit) = (bytes, 0);
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let decimals = if unit == 0 || value >= 100.0 { 0 } else { 1 };
    format!("{value:.decimals$} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_reads_like_the_qml_service() {
        let arch = "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\nHOME_URL=\"https://archlinux.org/\"\nLOGO=archlinux-logo\n";
        let bare = "NAME=\"Foo Linux\"\nID=\"cachyos\"\n";
        assert_eq!(
            parse_os_release(arch),
            OsRelease {
                name: "Arch Linux".to_owned(),
                home_url: "https://archlinux.org/".to_owned(),
                logo: "archlinux-logo".to_owned(),
                ..OsRelease::default()
            }
        );
        assert_eq!(
            parse_os_release(bare),
            OsRelease {
                name: "Foo".to_owned(),
                logo: "cachyos-symbolic".to_owned(),
                ..OsRelease::default()
            }
        );
        assert_eq!(parse_os_release("").name, "Unknown");
    }

    #[test]
    fn device_names_shorten_like_the_qml_service() {
        let cases = [
            ("Advanced Micro Devices, Inc. [AMD/ATI]", "AMD/ATI"),
            ("NVIDIA Corporation", "NVIDIA"),
            ("Realtek Semiconductor Co., Ltd.", "Realtek Semiconductor"),
            ("Shenzhen Foo Technology Co., Ltd.", "Shenzhen Foo"),
            ("Vendor 1234", "Vendor 1234"),
        ];
        for (name, expected) in cases {
            assert_eq!(short_device_name(name), expected);
        }
        let lspci = "00:01.0 \"ISA bridge\" \"Intel Corporation\" \"82371SB\"\n00:02.0 \"VGA compatible controller\" \"Vendor 1234\" \"Device 1111\" -r02\n01:00.0 \"3D controller\" \"NVIDIA Corporation\" \"GA107M [GeForce RTX 3050 Mobile]\"\n";
        assert_eq!(
            parse_graphics(lspci),
            "Vendor 1234 Device 1111, NVIDIA GeForce RTX 3050 Mobile"
        );
    }

    #[test]
    fn disks_keep_each_device_once() {
        let df = "Filesystem Mounted on Type Used Size\n/dev/vda3 / btrfs 7756988416 21158146048\ntmpfs /tmp tmpfs 4030464 3101921280\n/dev/vda3 /home btrfs 7756988416 21158146048\n/dev/vda2 /efi vfat 344064 313815040\n";
        let disks = parse_disks(df);
        let mounts: Vec<&str> = disks.iter().map(|disk| disk.mount.as_str()).collect();
        assert_eq!(mounts, ["/", "/efi"]);
        assert_eq!(disks[0].source, "vda3");
    }

    #[test]
    fn sizes_read_like_the_qml_service() {
        let cases = [
            (512.0, "512 B"),
            (313815040.0, "299 MiB"),
            (13401157632.0, "12.5 GiB"),
            (21158146048.0, "19.7 GiB"),
        ];
        for (bytes, expected) in cases {
            assert_eq!(human_size(bytes), expected);
        }
    }
}
