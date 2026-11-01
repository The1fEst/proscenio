use crate::core::{config, process};

const PROGRAM: &str = "flatpak";
const INSTALLATIONS: [Installation; 2] = [Installation::User, Installation::System];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Installation {
    User,
    System,
}

impl Installation {
    fn flag(self) -> &'static str {
        match self {
            Installation::User => "--user",
            Installation::System => "--system",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatpakUpdate {
    pub application: String,
    pub version: String,
    pub branch: String,
    pub origin: String,
    pub installation: Installation,
}

impl FlatpakUpdate {
    pub fn describe(&self) -> String {
        let version = if self.version.is_empty() {
            &self.branch
        } else {
            &self.version
        };
        [version.as_str(), self.origin.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

pub fn is_installed() -> bool {
    process::exists(PROGRAM)
}

pub fn is_enabled() -> bool {
    should_include(is_installed(), config::current().updates_include_flatpak)
}

fn should_include(is_installed: bool, is_setting_on: bool) -> bool {
    is_installed && is_setting_on
}

pub(super) async fn list_updates() -> Vec<FlatpakUpdate> {
    if !is_enabled() {
        return Vec::new();
    }
    let mut updates = Vec::new();
    let mut has_failed = false;
    for installation in INSTALLATIONS {
        let mut command = process::command(&build_query(installation));
        command.env("LC_ALL", "C").env_remove("LANGUAGE");
        match process::capture(command)
            .await
            .filter(|output| output.status.success())
        {
            Some(output) => updates.extend(parse_updates(
                &String::from_utf8_lossy(&output.stdout),
                installation,
            )),
            None => has_failed = true,
        }
    }
    if has_failed {
        eprintln!("{PROGRAM}: could not list the updates");
    }
    updates
}

fn build_query(installation: Installation) -> [&'static str; 5] {
    [
        PROGRAM,
        "remote-ls",
        "--updates",
        "--columns=application,version,branch,origin",
        installation.flag(),
    ]
}

fn parse_updates(output: &str, installation: Installation) -> Vec<FlatpakUpdate> {
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('\t').map(str::trim);
            let application = columns
                .next()
                .filter(|name| !name.is_empty() && !name.contains(char::is_whitespace))?;
            let mut next_column = || columns.next().unwrap_or_default().to_owned();
            Some(FlatpakUpdate {
                application: application.to_owned(),
                version: next_column(),
                branch: next_column(),
                origin: next_column(),
                installation,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_update(application: &str, version: &str, installation: Installation) -> FlatpakUpdate {
        FlatpakUpdate {
            application: application.to_owned(),
            version: version.to_owned(),
            branch: "stable".to_owned(),
            origin: "flathub".to_owned(),
            installation,
        }
    }

    #[test]
    fn a_listing_gives_one_update_per_tab_separated_line() {
        let output = "org.mozilla.firefox\t139.0\tstable\tflathub\n\
                      org.freedesktop.Platform\t\t24.08\tflathub\n";
        assert_eq!(
            parse_updates(output, Installation::System),
            [
                FlatpakUpdate {
                    version: "139.0".to_owned(),
                    ..build_update("org.mozilla.firefox", "", Installation::System)
                },
                FlatpakUpdate {
                    branch: "24.08".to_owned(),
                    ..build_update("org.freedesktop.Platform", "", Installation::System)
                },
            ]
        );
    }

    #[test]
    fn a_listing_without_updates_or_with_odd_lines_gives_only_the_real_ones() {
        assert!(parse_updates("", Installation::User).is_empty());
        assert!(parse_updates("\n  \n\t\n", Installation::User).is_empty());
        assert!(
            parse_updates(
                "Application ID\tVersion\tBranch\tOrigin\n",
                Installation::User
            )
            .is_empty()
        );
        assert!(parse_updates("Nothing to do.\n", Installation::User).is_empty());
        let lines = "org.a.App\n\
                     org.b.App\t1.0\n\
                     org.c.App\t1.0\tstable\tflathub\tx86_64\tmore\n";
        let updates = parse_updates(lines, Installation::User);
        assert_eq!(
            updates
                .iter()
                .map(|update| update.application.as_str())
                .collect::<Vec<_>>(),
            ["org.a.App", "org.b.App", "org.c.App"]
        );
        assert_eq!(updates[0].version, "");
        assert_eq!(updates[1].version, "1.0");
        assert_eq!(updates[1].origin, "");
        assert_eq!(updates[2].origin, "flathub");
        assert!(
            updates
                .iter()
                .all(|update| update.installation == Installation::User)
        );
    }

    #[test]
    fn an_update_reads_as_its_version_or_branch_and_its_remote() {
        let mut firefox = build_update("org.mozilla.firefox", "139.0", Installation::User);
        assert_eq!(firefox.describe(), "139.0 · flathub");
        firefox.version.clear();
        assert_eq!(firefox.describe(), "stable · flathub");
        firefox.origin.clear();
        assert_eq!(firefox.describe(), "stable");
        firefox.branch.clear();
        assert_eq!(firefox.describe(), "");
    }

    #[test]
    fn flatpak_is_used_only_when_it_is_installed_and_the_setting_is_on() {
        assert!(should_include(true, true));
        assert!(!should_include(true, false));
        assert!(!should_include(false, true));
        assert!(!should_include(false, false));
    }

    #[test]
    fn each_installation_is_listed_by_its_own_query() {
        for (installation, flag) in [
            (Installation::User, "--user"),
            (Installation::System, "--system"),
        ] {
            assert_eq!(
                build_query(installation),
                [
                    "flatpak",
                    "remote-ls",
                    "--updates",
                    "--columns=application,version,branch,origin",
                    flag
                ]
            );
        }
    }
}
