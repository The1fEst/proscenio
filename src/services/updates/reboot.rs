use std::io::{BufRead, BufReader};
use std::path::Path;

const CORE_PACKAGES: [&str; 2] = ["systemd", "glibc"];
const MODULES_DIRECTORY: &str = "usr/lib/modules/";
const SOURCES_DIRECTORY: &str = "usr/src/";
const MODULE_SUFFIXES: [&str; 4] = [".ko", ".ko.zst", ".ko.xz", ".ko.gz"];

pub(super) fn needs_reboot(name: &str, files: impl IntoIterator<Item = impl AsRef<str>>) -> bool {
    CORE_PACKAGES.contains(&name)
        || name.ends_with("-ucode")
        || name.ends_with("-dkms")
        || files
            .into_iter()
            .any(|file| is_kernel_file(file.as_ref()) || is_module_source(file.as_ref()))
}

fn is_kernel_file(file: &str) -> bool {
    let Some(path) = file.strip_prefix(MODULES_DIRECTORY) else {
        return false;
    };
    let Some((_release, path)) = path.split_once('/') else {
        return false;
    };
    path == "vmlinuz"
        || (!path.starts_with("build/")
            && MODULE_SUFFIXES.iter().any(|suffix| path.ends_with(suffix)))
}

fn is_module_source(file: &str) -> bool {
    file.strip_prefix(SOURCES_DIRECTORY)
        .and_then(|path| path.split_once('/'))
        .is_some_and(|(directory, file)| !directory.is_empty() && file == "dkms.conf")
}

pub(super) fn find_reboot_packages(
    local_database: &Path,
    installed_packages: &[(String, String)],
) -> Vec<String> {
    installed_packages
        .iter()
        .filter(|(name, version)| {
            if version.is_empty() {
                return false;
            }
            let files = std::fs::File::open(
                local_database
                    .join(format!("{name}-{version}"))
                    .join("files"),
            )
            .into_iter()
            .flat_map(|file| BufReader::new(file).split(b'\n').map_while(Result::ok))
            .map(|line| String::from_utf8_lossy(&line).into_owned())
            .skip_while(|line| line != "%FILES%")
            .skip(1)
            .take_while(|line| !line.is_empty());
            needs_reboot(name, files)
        })
        .map(|(name, _)| name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_that_the_running_system_cannot_swap_in_asks_for_a_reboot() {
        let cases: [(&str, &[&str], bool); 17] = [
            (
                "linux",
                &[
                    "usr/lib/modules/7.2.9-arch1-1/",
                    "usr/lib/modules/7.2.9-arch1-1/vmlinuz",
                ],
                true,
            ),
            (
                "linux-cachyos",
                &["usr/lib/modules/7.2.9-1-cachyos/kernel/fs/ext4/ext4.ko.zst"],
                true,
            ),
            (
                "nvidia-open",
                &["usr/lib/modules/7.2.9-arch1-1/extramodules/nvidia.ko.xz"],
                true,
            ),
            (
                "module-in-updates",
                &["usr/lib/modules/7.2.9/updates/a.ko"],
                true,
            ),
            ("driver-git", &["usr/src/driver-1.0/dkms.conf"], true),
            ("xone-dkms", &[], true),
            ("intel-ucode", &["boot/intel-ucode.img"], true),
            ("amd-ucode", &[], true),
            ("systemd", &[], true),
            ("glibc", &[], true),
            ("systemd-libs", &["usr/lib/libsystemd.so"], false),
            (
                "linux-headers",
                &[
                    "usr/lib/modules/7.2.9-arch1-1/build/Makefile",
                    "usr/lib/modules/7.2.9-arch1-1/build/tools/x.ko",
                ],
                false,
            ),
            ("mesa", &["usr/lib/libgbm.so"], false),
            ("firefox", &[], false),
            ("sources", &["usr/src/linux/README"], false),
            ("nested-sources", &["usr/src/a/b/dkms.conf"], false),
            ("modules-file", &["usr/lib/modules/notes.txt"], false),
        ];
        for (name, files, expected) in cases {
            assert_eq!(needs_reboot(name, files), expected, "{name}");
        }
    }

    #[test]
    fn the_files_of_the_installed_version_decide_and_only_for_upgrades() {
        let database =
            std::env::temp_dir().join(format!("proscenio-reboot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&database);
        std::fs::create_dir_all(database.join("linux-7.2.9-1")).unwrap();
        std::fs::write(
            database.join("linux-7.2.9-1/files"),
            b"%FILES%\nusr/\nusr/share/caf\xe9\nusr/lib/modules/7.2.9-arch1-1/vmlinuz\n\n%BACKUP%\netc/x\tabc\n",
        )
        .unwrap();
        std::fs::create_dir_all(database.join("mesa-1:25.0-1")).unwrap();
        std::fs::write(
            database.join("mesa-1:25.0-1/files"),
            "%FILES%\nusr/lib/libgbm.so\n",
        )
        .unwrap();
        let to_packages = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(name, version)| (name.to_string(), version.to_string()))
                .collect()
        };
        let found = find_reboot_packages(
            &database,
            &to_packages(&[
                ("linux", "7.2.9-1"),
                ("mesa", "1:25.0-1"),
                ("intel-ucode", "20260101-1"),
                ("amd-ucode", ""),
                ("missing", "1-1"),
            ]),
        );
        std::fs::remove_dir_all(&database).unwrap();
        assert_eq!(found, ["linux", "intel-ucode"]);
    }
}
