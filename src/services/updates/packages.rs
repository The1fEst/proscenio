use gtk4::glib;
use proscenio_protocol::{Change, Decoder, Event};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Output;

use super::manager::{aur_query, build_plan_query, package_manager, repository_query};
use super::output::{decode_events, stage_text, visible_text};
use super::{Job, Notice, Updates};
use crate::core::{paths, process};

pub(super) const LOCAL_DATABASE: &str = "/var/lib/pacman/local";
const AUR_NOTICES: [&str; 7] = [
    "warning:",
    "missing aur packages:",
    "missing aur debug packages:",
    "orphaned aur packages:",
    "orphan (unmaintained) aur packages:",
    "flagged out of date aur packages:",
    "packages not in aur:",
];

impl Updates {
    pub(super) fn refresh_packages(&self) {
        if !is_package_check_allowed(self.job.get()) || self.is_refreshing_packages.replace(true) {
            self.is_package_refresh_queued.set(true);
            return;
        }
        self.is_package_refresh_queued.set(false);
        self.listeners.notify();
        let updates = self.clone();
        glib::spawn_future_local(async move {
            updates.load_package_lists().await;
            updates.is_refreshing_packages.set(false);
            updates.listeners.notify();
            if updates.is_package_refresh_queued.replace(false) {
                updates.refresh_packages();
            }
        });
    }

    async fn load_package_lists(&self) {
        let manager = package_manager();
        let database = private_database_dir();
        let Some(manager) = manager.filter(|_| prepare(&database)) else {
            self.has_package_manager.set(manager.is_some());
            self.has_package_check_failed.set(manager.is_some());
            self.repository_packages.replace(Vec::new());
            self.aur_packages.replace(Vec::new());
            self.package_urls.replace(HashMap::new());
            self.listeners.notify();
            return;
        };
        let database = database.to_string_lossy().into_owned();
        let (repository_packages, package_urls) =
            query_pending_packages(&repository_query(&database), has_repository_check_failed)
                .await
                .map(split_package_urls)
                .unzip();
        let aur_packages = query_aur_packages(manager).await;
        self.has_package_manager.set(true);
        self.has_package_check_failed
            .set(repository_packages.is_none() || aur_packages.is_none());
        self.repository_packages
            .replace(repository_packages.unwrap_or_default());
        self.aur_packages.replace(aur_packages.unwrap_or_default());
        self.package_urls.replace(package_urls.unwrap_or_default());
        if let Some(notice) = self.notice.get()
            && contradicts_notice(notice, self.count(), self.has_package_check_failed.get())
        {
            self.notice.set(None);
        }
        self.listeners.notify();
    }

    pub(super) async fn load_system_plan(&self) -> Result<SystemPlan, String> {
        let manager =
            package_manager().ok_or_else(|| "no package manager is installed".to_owned())?;
        let database = private_database_dir();
        if !prepare(&database) {
            return Err(format!("could not prepare {}", database.display()));
        }
        let database = database.to_string_lossy().into_owned();
        let query = build_plan_query(&database);
        self.push_log(&format!("$ {}", query.join(" ")));
        let aur_listing = glib::spawn_future_local(query_aur_packages(manager));
        let output = process::capture(process::command(&query))
            .await
            .ok_or_else(|| format!("could not run {}", query[0]))?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(if message.is_empty() {
                format!("{} exited with {}", query[0], output.status)
            } else {
                message
            });
        }
        let (changes, log_lines) = parse_plan(&output.stdout)
            .ok_or_else(|| format!("could not read the plan of {}", query[0]))?;
        Ok(SystemPlan {
            changes,
            aur_packages: aur_listing.await.ok().flatten(),
            log_lines,
        })
    }

    pub(super) fn show_plan(&self, changes: &[Change], aur_packages: Vec<String>) {
        self.has_package_manager.set(true);
        self.has_package_check_failed.set(false);
        self.repository_packages
            .replace(list_upgrade_lines(changes));
        self.aur_packages.replace(aur_packages);
        self.package_urls.replace(list_package_urls(changes));
        self.listeners.notify();
    }
}

pub(super) async fn query_aur_packages(manager: &str) -> Option<Vec<String>> {
    match aur_query(manager) {
        Some(query) => query_pending_packages(&query, has_aur_query_failed).await,
        None => Some(Vec::new()),
    }
}

pub(super) struct SystemPlan {
    pub(super) changes: Vec<Change>,
    pub(super) aur_packages: Option<Vec<String>>,
    pub(super) log_lines: Vec<String>,
}

pub(super) fn private_database_dir() -> PathBuf {
    paths::cache().join("pacman")
}

fn parse_plan(stdout: &[u8]) -> Option<(Vec<Change>, Vec<String>)> {
    let mut decoder = Decoder::default();
    let messages = decode_events(&mut decoder, Some(stdout))
        .into_iter()
        .chain(decode_events(&mut decoder, None));
    let mut changes = None;
    let mut lines = Vec::new();
    for message in messages {
        match message.ok()? {
            Event::Planned(planned) => changes = Some(planned),
            Event::Stage(stage) => lines.push(stage_text(stage).to_owned()),
            Event::Line(line) => lines.push(visible_text(&line)),
            _ => {}
        }
    }
    Some((changes?, lines))
}

fn is_package_check_allowed(running_job: Option<Job>) -> bool {
    running_job != Some(Job::System)
}

fn contradicts_notice(notice: Notice, count: i32, has_check_failed: bool) -> bool {
    match notice {
        Notice::UpToDate => count > 0,
        Notice::CheckFailed => !has_check_failed,
    }
}

fn list_upgrade_lines(changes: &[Change]) -> Vec<String> {
    changes
        .iter()
        .filter(|change| !change.from.is_empty() && !change.to.is_empty())
        .map(|change| format!("{} {} -> {}", change.name, change.from, change.to))
        .collect()
}

async fn query_pending_packages(
    query: &[&str],
    has_failed: fn(&Output) -> bool,
) -> Option<Vec<String>> {
    let mut command = process::command(query);
    command.env("LC_ALL", "C").env_remove("LANGUAGE");
    let output = process::capture(command).await?;
    if has_failed(&output) {
        return None;
    }
    Some(parse_pending_packages(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn has_repository_check_failed(output: &Output) -> bool {
    !output.status.success()
}

fn has_aur_query_failed(output: &Output) -> bool {
    if output.status.success() {
        return false;
    }
    if output.status.code() != Some(1) {
        return true;
    }
    String::from_utf8_lossy(&output.stderr).lines().any(|line| {
        let text = visible_text(line).trim().to_ascii_lowercase();
        let message = text.trim_start_matches(['-', '>', ' ', ':', '=']);
        let is_error = message.starts_with("error:") || message.starts_with("error ");
        let is_progress =
            (text.starts_with("::") || text.starts_with("==>")) && text.ends_with("...");
        let is_package_notice = message.split_once(": ").is_some_and(|(name, rest)| {
            !name.contains(' ')
                && (rest.starts_with("ignoring package upgrade (")
                    || (rest.starts_with("local (") && rest.contains(") is newer than aur (")))
        });
        let is_notice = is_progress
            || is_package_notice
            || AUR_NOTICES.iter().any(|notice| message.starts_with(notice));
        !text.is_empty() && (is_error || !is_notice)
    })
}

fn prepare(database: &Path) -> bool {
    let local = database.join("local");
    std::fs::create_dir_all(database).is_ok()
        && (local.exists() || std::os::unix::fs::symlink(LOCAL_DATABASE, &local).is_ok())
}

fn parse_pending_packages(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| version_change(line).is_some() && !line.ends_with("[ignored]"))
        .map(str::to_owned)
        .collect()
}

pub fn version_change(line: &str) -> Option<(&str, &str, &str)> {
    let (name, versions) = line.split_once(' ')?;
    let (from, to) = versions.split_once(" -> ")?;
    Some((name, from, to))
}

pub fn parse_web_url(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    let is_web = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
    let has_blank = url
        .chars()
        .any(|character| character.is_whitespace() || character.is_control());
    (is_web && !rest.is_empty() && !has_blank).then_some(url)
}

pub fn build_aur_page_url(name: &str) -> Option<String> {
    let is_name = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "@._+-".contains(character));
    is_name.then(|| format!("https://aur.archlinux.org/packages/{name}"))
}

fn split_package_urls(lines: Vec<String>) -> (Vec<String>, HashMap<String, String>) {
    let mut urls = HashMap::new();
    let lines = lines
        .into_iter()
        .map(|line| {
            let Some((text, url)) = line.split_once('\t') else {
                return line;
            };
            if let Some((name, _, _)) = version_change(text)
                && !url.is_empty()
            {
                urls.insert(name.to_owned(), url.to_owned());
            }
            text.to_owned()
        })
        .collect();
    (lines, urls)
}

fn list_package_urls(changes: &[Change]) -> HashMap<String, String> {
    changes
        .iter()
        .filter_map(|change| Some((change.name.clone(), change.url.clone()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proscenio_protocol::{Message, Stage};

    #[test]
    fn the_pending_packages_are_the_upgradable_ones_but_not_the_ignored_ones() {
        let output = "appstream 1.2.0-1 -> 1.2.1-1\n\
                      linux 7.2.8-1 -> 7.2.9-1 [ignored]\n\
                      zen-browser-bin 1.22.3b-1 -> 1.23b-1\n\
                      :: Searching AUR for updates...\n\
                      \n";
        assert_eq!(
            parse_pending_packages(output),
            [
                "appstream 1.2.0-1 -> 1.2.1-1",
                "zen-browser-bin 1.22.3b-1 -> 1.23b-1"
            ]
        );
        assert!(parse_pending_packages("").is_empty());
    }

    #[test]
    fn a_package_query_tells_nothing_pending_from_a_failure() {
        let context = glib::MainContext::new();
        let query = |script: &str, has_failed: fn(&Output) -> bool| {
            context.block_on(query_pending_packages(&["sh", "-c", script], has_failed))
        };
        let upgrade = Some(vec!["paru 2.0.4-1 -> 2.1.0-1".to_owned()]);
        let script = "echo 'paru 2.0.4-1 -> 2.1.0-1'";
        assert_eq!(query(script, has_repository_check_failed), upgrade);
        assert_eq!(query(script, has_aur_query_failed), upgrade);
        assert_eq!(query("exit 1", has_repository_check_failed), None);
        for script in [
            "exit 1",
            "echo 'a 1-1 -> 2-1 [ignored]'; exit 1",
            "echo 'warning: devel lookup skipped' >&2; exit 1",
            "printf '\\033[1;33mwarning:\\033[0m skipped\\n' >&2; exit 1",
            "echo ':: Looking for devel upgrades...' >&2; exit 1",
            "echo ' -> Missing AUR Packages:  fEst-proscenio' >&2; exit 1",
            "echo ' -> Packages not in AUR:  fEst-proscenio' >&2; exit 1",
            "echo ' -> Orphan (unmaintained) AUR Packages:  foo' >&2; exit 1",
            "echo ' -> Missing AUR Debug Packages:  foo-debug' >&2; exit 1",
            "echo ' -> errorlens-git: local (r120) is newer than AUR (r100)' >&2; exit 1",
            "echo 'warning: perl-error: ignoring package upgrade (1-1 => 2-1)' >&2; exit 1",
            "echo ' -> foo: ignoring package upgrade (1-1 => 2-1)' >&2; exit 1",
        ] {
            assert_eq!(
                query(script, has_aur_query_failed),
                Some(Vec::new()),
                "{script}"
            );
        }
        for message in [
            "Error: error sending request",
            " -> Get \"https://aur.archlinux.org/rpc\": dial tcp: no such host",
            "==> ERROR: Failure while downloading foo",
            " -> Failed to query AUR packages: timeout",
            " -> warning-tool-git: could not fetch",
            ":: Unable to reach aur.archlinux.org",
            " -> could not resolve foo: local (r1) is newer than AUR (r0)",
            "error: failed to fetch devel info for foo-warning-git",
        ] {
            assert_eq!(
                query(
                    &format!("echo '{message}' >&2; exit 1"),
                    has_aur_query_failed
                ),
                None,
                "{message}"
            );
        }
        assert_eq!(query("exit 2", has_aur_query_failed), None);
        assert_eq!(
            context.block_on(query_pending_packages(
                &["/nonexistent/paru", "-Qua"],
                has_aur_query_failed
            )),
            None
        );
    }

    #[test]
    fn a_plan_lists_as_upgrades_only_the_packages_that_change_version() {
        let changes = [
            Change::new("appstream", "1.2.0-1", "1.2.1-1"),
            Change::new("libnew", "", "1.0-1"),
            Change::new("libgone", "2.0-1", ""),
        ];
        let lines = list_upgrade_lines(&changes);
        assert_eq!(lines, ["appstream 1.2.0-1 -> 1.2.1-1"]);
        assert_eq!(parse_pending_packages(&lines.join("\n")), lines);
        assert!(list_upgrade_lines(&[]).is_empty());
    }

    #[test]
    fn a_plan_is_read_from_the_helper_frames_with_the_lines_for_the_log() {
        let changes = vec![
            Change::new("linux", "7.2.9-1", "7.2.10-1"),
            Change::new("libnew", "", "1.0-1"),
            Change::new("oldlib", "2.0-1", ""),
        ];
        let mut stdout = Event::Stage(Stage::Synchronize).encode();
        stdout.extend(Event::Line("warning: skipped \x1b[0m".to_owned()).encode());
        stdout.extend(
            Event::Download {
                received_bytes: 1,
                total_bytes: 2,
            }
            .encode(),
        );
        stdout.extend(Event::Planned(changes.clone()).encode());
        assert_eq!(
            parse_plan(&stdout),
            Some((
                changes.clone(),
                vec![
                    ":: Synchronizing package databases...".to_owned(),
                    "warning: skipped ".to_owned()
                ]
            ))
        );
        assert_eq!(
            parse_plan(&Event::Planned(Vec::new()).encode()),
            Some((Vec::new(), Vec::new()))
        );

        let frame = Event::Planned(changes).encode();
        assert_eq!(parse_plan(&frame[..frame.len() - 1]), None);
        assert_eq!(parse_plan(&Event::Stage(Stage::Prepare).encode()), None);
        assert_eq!(parse_plan(&[1, 0, 0, 0, 99]), None);
        assert_eq!(parse_plan(&[]), None);
    }

    #[test]
    fn no_package_check_starts_while_a_system_update_runs() {
        assert!(is_package_check_allowed(None));
        assert!(is_package_check_allowed(Some(Job::Shell)));
        assert!(!is_package_check_allowed(Some(Job::System)));
    }

    #[test]
    fn a_later_check_replaces_a_notice_it_contradicts() {
        assert!(contradicts_notice(Notice::UpToDate, 3, false));
        assert!(!contradicts_notice(Notice::UpToDate, 0, false));
        assert!(contradicts_notice(Notice::CheckFailed, 0, false));
        assert!(contradicts_notice(Notice::CheckFailed, 3, false));
        assert!(!contradicts_notice(Notice::CheckFailed, 0, true));
    }

    #[test]
    fn only_web_addresses_get_a_link() {
        for url in [
            "http://example.org",
            "https://github.com/CachyOS/linux-cachyos",
        ] {
            assert_eq!(parse_web_url(url), Some(url));
        }
        assert_eq!(
            parse_web_url("HTTPS://Example.org/a"),
            Some("HTTPS://Example.org/a")
        );
        for url in [
            "",
            "https://",
            "ftp://example.org",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "example.org",
            "mailto:a@example.org",
            "https://example.org/a b",
            "https://example.org/\n",
            "https://example.org/\t",
            " https://example.org",
        ] {
            assert_eq!(parse_web_url(url), None, "{url:?}");
        }
    }

    #[test]
    fn an_aur_package_links_to_its_page_on_the_aur() {
        assert_eq!(
            build_aur_page_url("zen-browser-bin").as_deref(),
            Some("https://aur.archlinux.org/packages/zen-browser-bin")
        );
        assert_eq!(
            build_aur_page_url("libc++-git@2.0_1").as_deref(),
            Some("https://aur.archlinux.org/packages/libc++-git@2.0_1")
        );
        for name in ["", "a b", "a/b", "a?b", "a#b", "a%20b", "ü"] {
            assert_eq!(build_aur_page_url(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_check_line_gives_up_its_url_to_a_map_by_package_name() {
        let lines = [
            "linux-cachyos 7.2.9-1 -> 7.2.10-1\thttps://github.com/CachyOS/linux-cachyos",
            "libfoo 1.0-1 -> 1.1-1",
            "libbar 1.0-1 -> 1.1-1\t",
        ]
        .map(str::to_owned)
        .to_vec();
        let (lines, urls) = split_package_urls(lines);
        assert_eq!(
            lines,
            [
                "linux-cachyos 7.2.9-1 -> 7.2.10-1",
                "libfoo 1.0-1 -> 1.1-1",
                "libbar 1.0-1 -> 1.1-1"
            ]
        );
        assert_eq!(
            urls,
            HashMap::from([(
                "linux-cachyos".to_owned(),
                "https://github.com/CachyOS/linux-cachyos".to_owned()
            )])
        );
        assert_eq!(split_package_urls(Vec::new()), (Vec::new(), HashMap::new()));
    }

    #[test]
    fn a_plan_gives_its_urls_to_a_map_by_package_name() {
        let changes = [
            Change::new("linux", "7.2.9-1", "7.2.10-1").with_url(Some("https://kernel.org")),
            Change::new("libnew", "", "1.0-1").with_url(Some("https://new.org")),
            Change::new("libfoo", "1.0-1", "1.1-1"),
        ];
        assert_eq!(
            list_package_urls(&changes),
            HashMap::from([
                ("linux".to_owned(), "https://kernel.org".to_owned()),
                ("libnew".to_owned(), "https://new.org".to_owned())
            ])
        );
    }

    #[test]
    fn an_update_line_splits_into_name_and_both_versions() {
        assert_eq!(
            version_change("brave-origin-bin 1:1.96.61-1 -> 1:1.97.56-1"),
            Some(("brave-origin-bin", "1:1.96.61-1", "1:1.97.56-1"))
        );
        assert_eq!(version_change("garbage"), None);
    }
}
