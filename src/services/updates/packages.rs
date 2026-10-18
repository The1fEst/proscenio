use gtk4::glib;
use proscenio_protocol::Change;
use std::path::Path;
use std::process::Output;

use super::manager::{aur_query, package_manager, repository_query};
use super::output::visible_text;
use super::{Notice, Updates};
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
        if self.is_refreshing_packages.replace(true) {
            self.is_package_refresh_queued.set(true);
            return;
        }
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
        let database = paths::cache().join("pacman");
        let Some(manager) = manager.filter(|_| prepare(&database)) else {
            self.has_package_manager.set(manager.is_some());
            self.has_package_check_failed.set(manager.is_some());
            self.repository_packages.replace(Vec::new());
            self.aur_packages.replace(Vec::new());
            self.listeners.notify();
            return;
        };
        let database = database.to_string_lossy().into_owned();
        let repository_packages =
            query_pending_packages(&repository_query(&database), has_repository_check_failed).await;
        let aur_packages = query_aur_packages(manager).await;
        self.has_package_manager.set(true);
        self.has_package_check_failed
            .set(repository_packages.is_none() || aur_packages.is_none());
        self.repository_packages
            .replace(repository_packages.unwrap_or_default());
        self.aur_packages.replace(aur_packages.unwrap_or_default());
        if let Some(notice) = self.notice.get()
            && contradicts_notice(notice, self.count(), self.has_package_check_failed.get())
        {
            self.notice.set(None);
        }
        self.listeners.notify();
    }

    pub(super) fn show_plan(&self, changes: &[Change], aur_packages: Vec<String>) {
        self.has_package_manager.set(true);
        self.has_package_check_failed.set(false);
        self.repository_packages
            .replace(list_upgrade_lines(changes));
        self.aur_packages.replace(aur_packages);
        self.listeners.notify();
    }
}

pub(super) async fn query_aur_packages(manager: &str) -> Option<Vec<String>> {
    match aur_query(manager) {
        Some(query) => query_pending_packages(&query, has_aur_query_failed).await,
        None => Some(Vec::new()),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_later_check_replaces_a_notice_it_contradicts() {
        assert!(contradicts_notice(Notice::UpToDate, 3, false));
        assert!(!contradicts_notice(Notice::UpToDate, 0, false));
        assert!(contradicts_notice(Notice::CheckFailed, 0, false));
        assert!(contradicts_notice(Notice::CheckFailed, 3, false));
        assert!(!contradicts_notice(Notice::CheckFailed, 0, true));
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
