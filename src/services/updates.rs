use gtk4::glib;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::{config, paths, process, watch};

pub const REPOSITORY: &str = "The1fEst/proscenio";
const BRANCH: &str = "main";
const VERSION: &str = env!("PROSCENIO_VERSION");
const LOCAL_DATABASE: &str = "/var/lib/pacman/local";
const PACKAGE_MANAGERS: [&str; 3] = ["paru", "yay", "pacman"];

#[derive(Clone)]
pub struct Updates {
    pub count: Rc<Cell<i32>>,
    pub available: Rc<Cell<bool>>,
    pub behind: Rc<Cell<i32>>,
    listeners: Rc<Listeners>,
    following: Rc<RefCell<Option<watch::Watch>>>,
}

impl Updates {
    pub fn new() -> Self {
        let updates = Updates {
            count: Rc::new(Cell::new(0)),
            available: Rc::new(Cell::new(false)),
            behind: Rc::new(Cell::new(0)),
            listeners: Rc::default(),
            following: Rc::default(),
        };
        let checker = updates.clone();
        updates
            .following
            .replace(Some(watch::config("/updates/enableCheck", move || {
                if config::current().updates_enable_check {
                    checker.refresh();
                }
            })));
        updates
    }

    pub fn period() -> Duration {
        let config = config::current();
        if !config.updates_enable_check {
            return Duration::ZERO;
        }
        Duration::from_secs((config.updates_interval as u64).max(1) * 60)
    }

    pub fn advised(&self) -> bool {
        self.available.get() && self.count.get() >= config::current().updates_advise
    }

    pub fn strongly_advised(&self) -> bool {
        self.available.get() && self.count.get() >= config::current().updates_strongly_advise
    }

    pub fn shell_behind(&self) -> bool {
        self.behind.get() > 0
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn refresh(&self) {
        self.refresh_packages();
        self.refresh_shell();
    }

    fn refresh_shell(&self) {
        let Some(commit) = installed_commit(VERSION) else {
            return;
        };
        let url = format!("https://api.github.com/repos/{REPOSITORY}/compare/{commit}...{BRANCH}");
        let updates = self.clone();
        glib::spawn_future_local(async move {
            let comparison = process::capture_text(process::command(&[
                "curl",
                "-sf",
                "-H",
                "Accept: application/vnd.github+json",
                &url,
            ]))
            .await
            .and_then(|body| serde_json::from_str::<Value>(&body).ok());
            let behind = comparison.as_ref().and_then(commits_behind).unwrap_or(0);
            if updates.behind.replace(behind) != behind {
                updates.listeners.notify();
            }
        });
    }

    fn refresh_packages(&self) {
        let updates = self.clone();
        glib::spawn_future_local(async move {
            let manager = PACKAGE_MANAGERS
                .into_iter()
                .find(|program| glib::find_program_in_path(program).is_some());
            updates.available.set(manager.is_some());
            let database = paths::cache().join("pacman");
            let Some(manager) = manager.filter(|_| prepare(&database)) else {
                updates.listeners.notify();
                return;
            };
            let database = database.to_string_lossy().into_owned();
            let _ = process::capture(process::quiet(&[
                "unshare",
                "-r",
                "pacman",
                "-Sy",
                "--disable-sandbox",
                "--dbpath",
                &database,
                "--logfile",
                "/dev/null",
            ]))
            .await;
            let Some(output) =
                process::capture_text(process::command(&[manager, "-Qu", "--dbpath", &database]))
                    .await
            else {
                return;
            };
            updates.count.set(pending(&output));
            updates.listeners.notify();
        });
    }
}

fn prepare(database: &Path) -> bool {
    let local = database.join("local");
    std::fs::create_dir_all(database).is_ok()
        && (local.exists() || std::os::unix::fs::symlink(LOCAL_DATABASE, &local).is_ok())
}

fn pending(output: &str) -> i32 {
    output
        .lines()
        .filter(|line| !line.is_empty() && !line.ends_with("[ignored]"))
        .count() as i32
}

fn installed_commit(version: &str) -> Option<&str> {
    let (count, hash) = version.strip_prefix('r')?.split_once('.')?;
    let numeric = !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit());
    (numeric && !hash.is_empty()).then_some(hash)
}

fn commits_behind(comparison: &Value) -> Option<i32> {
    comparison
        .get("ahead_by")?
        .as_i64()
        .map(|commits| commits as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_commit_is_the_hash_in_the_build_version() {
        assert_eq!(installed_commit("r204.e28f698"), Some("e28f698"));
        assert_eq!(installed_commit("0.1.0"), None);
        assert_eq!(installed_commit("r12."), None);
    }

    #[test]
    fn the_shell_is_as_far_behind_as_main_is_ahead_of_its_commit() {
        let comparison = |json: &str| commits_behind(&serde_json::from_str(json).unwrap());
        assert_eq!(
            comparison(r#"{"status":"ahead","ahead_by":3,"behind_by":0}"#),
            Some(3)
        );
        assert_eq!(
            comparison(r#"{"status":"identical","ahead_by":0,"behind_by":0}"#),
            Some(0)
        );
        assert_eq!(
            comparison(r#"{"status":"diverged","ahead_by":2,"behind_by":5}"#),
            Some(2)
        );
        assert_eq!(comparison(r#"{"message":"Not Found"}"#), None);
    }

    #[test]
    fn pending_counts_the_upgradable_packages_listed_but_not_the_ignored_ones() {
        let output = "appstream 1.2.0-1 -> 1.2.1-1\n\
                      linux 7.2.8-1 -> 7.2.9-1 [ignored]\n\
                      zen-browser-bin 1.22.3b-1 -> 1.23b-1\n\
                      \n";
        assert_eq!(pending(output), 2);
        assert_eq!(pending(""), 0);
    }
}
