use gtk4::glib;

use super::HELPER;

const PACKAGE_MANAGERS: [&str; 3] = ["paru", "yay", "pacman"];

pub(super) fn package_manager() -> Option<&'static str> {
    PACKAGE_MANAGERS
        .into_iter()
        .find(|program| glib::find_program_in_path(program).is_some())
}

pub(super) fn repository_query(database: &str) -> [&str; 3] {
    [HELPER, "check", database]
}

pub(super) fn build_plan_query(database: &str) -> [&str; 3] {
    [HELPER, "plan", database]
}

pub(super) fn aur_query(manager: &str) -> Option<[&str; 2]> {
    matches!(manager, "paru" | "yay").then_some([manager, "-Qua"])
}

pub fn install_command<'a>(packages: &[&'a str]) -> Option<Vec<&'a str>> {
    Some(unattended_command(package_manager()?, "-S", packages))
}

pub(super) fn aur_upgrade<'a>(manager: &'a str, packages: &[&'a str]) -> Vec<&'a str> {
    let mut command: Vec<&str> = unattended_command(manager, "-Sa", &[])
        .into_iter()
        .map(|argument| {
            if argument == "pkexec" {
                HELPER
            } else {
                argument
            }
        })
        .collect();
    command.extend_from_slice(packages);
    command
}

fn unattended_command<'a>(
    manager: &'a str,
    operation: &'a str,
    packages: &[&'a str],
) -> Vec<&'a str> {
    let mut line = match manager {
        "paru" => vec![
            "paru",
            operation,
            "--noconfirm",
            "--skipreview",
            "--batchinstall",
            "--sudo",
            "pkexec",
            "--nosudoloop",
        ],
        "yay" => vec![
            "yay",
            operation,
            "--noconfirm",
            "--answerclean",
            "None",
            "--answerdiff",
            "None",
            "--sudo",
            "pkexec",
            "--nosudoloop",
        ],
        _ => vec!["pkexec", "pacman", operation, "--noconfirm"],
    };
    line.extend_from_slice(packages);
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aur_upgrade_builds_only_the_confirmed_packages_with_the_helper_in_place_of_sudo() {
        for manager in ["paru", "yay"] {
            let command = aur_upgrade(manager, &["zen-browser-bin", "foo-git"]);
            assert_eq!(command[..2], [manager, "-Sa"]);
            assert!(!command.contains(&"-Sua"), "{manager}");
            assert!(command.contains(&"--noconfirm"), "{manager}");
            assert!(
                command.windows(2).any(|pair| pair == ["--sudo", HELPER]),
                "{manager}"
            );
            assert!(!command.contains(&"pkexec"), "{manager}");
            assert_eq!(command[command.len() - 2..], ["zen-browser-bin", "foo-git"]);
        }
        assert!(!aur_upgrade("paru", &["pkexec"]).ends_with(&[HELPER]));
    }

    #[test]
    fn an_install_runs_like_an_upgrade_with_the_packages_at_the_end() {
        assert_eq!(
            unattended_command("pacman", "-S", &["mpvpaper", "ffmpeg"]),
            [
                "pkexec",
                "pacman",
                "-S",
                "--noconfirm",
                "mpvpaper",
                "ffmpeg"
            ]
        );
        let paru = unattended_command("paru", "-S", &["upscayl-bin"]);
        assert_eq!(paru[..2], ["paru", "-S"]);
        assert_eq!(
            paru[2..paru.len() - 1],
            unattended_command("paru", "-Syu", &[])[2..]
        );
        assert_eq!(paru.last(), Some(&"upscayl-bin"));
    }

    #[test]
    fn repository_updates_come_from_the_helper_so_aur_packages_are_not_listed_twice() {
        assert_eq!(
            repository_query("/tmp/db"),
            ["/usr/lib/proscenio/proscenio-packages", "check", "/tmp/db"]
        );
    }

    #[test]
    fn the_repository_plan_comes_from_the_helper_without_root() {
        assert_eq!(
            build_plan_query("/tmp/db"),
            ["/usr/lib/proscenio/proscenio-packages", "plan", "/tmp/db"]
        );
    }

    #[test]
    fn aur_updates_are_listed_by_the_aur_helpers_only() {
        assert_eq!(aur_query("paru"), Some(["paru", "-Qua"]));
        assert_eq!(aur_query("yay"), Some(["yay", "-Qua"]));
        assert_eq!(aur_query("pacman"), None);
    }
}
