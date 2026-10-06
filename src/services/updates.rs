use gtk4::glib;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::process::Stdio;
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::{config, paths, process, watch};

pub const REPOSITORY: &str = "The1fEst/proscenio";
const BRANCH: &str = "main";
const VERSION: &str = env!("PROSCENIO_VERSION");
const LOCAL_DATABASE: &str = "/var/lib/pacman/local";
const PACKAGE_MANAGERS: [&str; 3] = ["paru", "yay", "pacman"];
const MERGED_OUTPUT: &str = "exec 2>&1; exec \"$@\"";
const SHELL_UPDATE: &str = "exec 2>&1
set -e
mkdir -p \"$1\"
cd \"$1\"
curl -fsSLO \"$2\"
PACMAN_AUTH=pkexec makepkg -Acfsi --noconfirm
systemctl --user restart proscenio";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Job {
    System,
    Shell,
}

#[derive(Clone)]
pub struct Updates {
    pub packages: Rc<RefCell<Vec<String>>>,
    pub available: Rc<Cell<bool>>,
    pub behind: Rc<Cell<i32>>,
    pub commits: Rc<RefCell<Vec<String>>>,
    pub job: Rc<Cell<Option<Job>>>,
    pub succeeded: Rc<Cell<Option<bool>>>,
    pub log: Rc<RefCell<String>>,
    listeners: Rc<Listeners>,
    output: Rc<Listeners>,
    following: Rc<RefCell<Option<watch::Watch>>>,
}

impl Updates {
    pub fn new() -> Self {
        let updates = Updates {
            packages: Rc::default(),
            available: Rc::new(Cell::new(false)),
            behind: Rc::new(Cell::new(0)),
            commits: Rc::default(),
            job: Rc::default(),
            succeeded: Rc::default(),
            log: Rc::default(),
            listeners: Rc::default(),
            output: Rc::default(),
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

    pub fn count(&self) -> i32 {
        self.packages.borrow().len() as i32
    }

    pub fn advised(&self) -> bool {
        self.available.get() && self.count() >= config::current().updates_advise
    }

    pub fn strongly_advised(&self) -> bool {
        self.available.get() && self.count() >= config::current().updates_strongly_advise
    }

    pub fn shell_behind(&self) -> bool {
        self.behind.get() > 0
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn subscribe_output(&self, listener: impl Fn() + 'static) -> Subscription {
        self.output.add(listener)
    }

    pub fn refresh(&self) {
        self.refresh_packages();
        self.refresh_shell();
    }

    pub fn start(&self, job: Job) {
        if self.job.get().is_some() {
            return;
        }
        let line: Vec<String> = match job {
            Job::System => {
                let Some(manager) = package_manager() else {
                    return;
                };
                ["bash", "-c", MERGED_OUTPUT, "bash"]
                    .into_iter()
                    .chain(upgrade_command(manager))
                    .map(str::to_owned)
                    .collect()
            }
            Job::Shell => vec![
                "bash".to_owned(),
                "-c".to_owned(),
                SHELL_UPDATE.to_owned(),
                "bash".to_owned(),
                paths::cache()
                    .join("package")
                    .to_string_lossy()
                    .into_owned(),
                format!(
                    "https://raw.githubusercontent.com/{REPOSITORY}/{BRANCH}/packaging/PKGBUILD"
                ),
            ],
        };
        let mut command = process::own_scope(None, &line);
        command.stdout(Stdio::piped());
        let Ok(mut child) = command.spawn() else {
            return;
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        self.log.borrow_mut().clear();
        self.succeeded.set(None);
        self.job.set(Some(job));
        self.listeners.notify();
        self.output.notify();

        let updates = self.clone();
        process::lines(stdout, move |line| {
            let Some(line) = line else {
                return;
            };
            {
                let mut log = updates.log.borrow_mut();
                log.push_str(line);
                log.push('\n');
            }
            updates.output.notify();
        });
        let updates = self.clone();
        let pid = glib::Pid(child.id() as i32);
        glib::spawn_future_local(async move {
            let (_, status) = glib::child_watch_future(pid).await;
            updates.job.set(None);
            updates.succeeded.set(Some(status == 0));
            updates.listeners.notify();
            updates.refresh();
        });
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
            updates.behind.set(behind);
            updates
                .commits
                .replace(comparison.as_ref().map(new_commits).unwrap_or_default());
            updates.listeners.notify();
        });
    }

    fn refresh_packages(&self) {
        let updates = self.clone();
        glib::spawn_future_local(async move {
            let manager = package_manager();
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
            updates.packages.replace(pending(&output));
            updates.listeners.notify();
        });
    }
}

fn package_manager() -> Option<&'static str> {
    PACKAGE_MANAGERS
        .into_iter()
        .find(|program| glib::find_program_in_path(program).is_some())
}

fn prepare(database: &Path) -> bool {
    let local = database.join("local");
    std::fs::create_dir_all(database).is_ok()
        && (local.exists() || std::os::unix::fs::symlink(LOCAL_DATABASE, &local).is_ok())
}

fn pending(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| !line.is_empty() && !line.ends_with("[ignored]"))
        .map(str::to_owned)
        .collect()
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

fn new_commits(comparison: &Value) -> Vec<String> {
    comparison
        .get("commits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|commit| commit.pointer("/commit/message")?.as_str())
        .map(|message| message.lines().next().unwrap_or_default().to_owned())
        .collect()
}

fn upgrade_command(manager: &str) -> Vec<&'static str> {
    match manager {
        "paru" => vec![
            "paru",
            "-Syu",
            "--noconfirm",
            "--skipreview",
            "--batchinstall",
            "--sudo",
            "pkexec",
            "--nosudoloop",
        ],
        "yay" => vec![
            "yay",
            "-Syu",
            "--noconfirm",
            "--answerclean",
            "None",
            "--answerdiff",
            "None",
            "--sudo",
            "pkexec",
            "--nosudoloop",
        ],
        _ => vec!["pkexec", "pacman", "-Syu", "--noconfirm"],
    }
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
    fn the_new_commits_are_the_subjects_of_the_compared_commits() {
        let comparison = serde_json::from_str(
            r#"{"ahead_by":2,"commits":[
                {"commit":{"message":"Theme: redraw every widget\n\nBody"}},
                {"commit":{"message":"Docs: links"}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            new_commits(&comparison),
            ["Theme: redraw every widget", "Docs: links"]
        );
        assert!(
            new_commits(&serde_json::from_str(r#"{"message":"Not Found"}"#).unwrap()).is_empty()
        );
    }

    #[test]
    fn pending_lists_the_upgradable_packages_but_not_the_ignored_ones() {
        let output = "appstream 1.2.0-1 -> 1.2.1-1\n\
                      linux 7.2.8-1 -> 7.2.9-1 [ignored]\n\
                      zen-browser-bin 1.22.3b-1 -> 1.23b-1\n\
                      \n";
        assert_eq!(
            pending(output),
            [
                "appstream 1.2.0-1 -> 1.2.1-1",
                "zen-browser-bin 1.22.3b-1 -> 1.23b-1"
            ]
        );
        assert!(pending("").is_empty());
    }

    #[test]
    fn every_system_upgrade_runs_unattended_and_asks_for_the_password_through_polkit() {
        for manager in PACKAGE_MANAGERS {
            let command = upgrade_command(manager);
            assert!(command.contains(&"-Syu"), "{manager}");
            assert!(command.contains(&"--noconfirm"), "{manager}");
            let polkit = command.first() == Some(&"pkexec")
                || command.windows(2).any(|pair| pair == ["--sudo", "pkexec"]);
            assert!(polkit, "{manager}");
        }
        assert_eq!(upgrade_command("paru")[0], "paru");
        assert_eq!(upgrade_command("yay")[0], "yay");
    }
}
