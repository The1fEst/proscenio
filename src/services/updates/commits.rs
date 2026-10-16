use gtk4::glib;
use serde_json::Value;

use super::{BRANCH, REPOSITORY, Updates};
use crate::core::process;

const VERSION: &str = env!("PROSCENIO_VERSION");
const SHORT_SHA_LENGTH: usize = 7;

#[derive(Debug, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
}

impl Commit {
    pub fn short_sha(&self) -> &str {
        self.sha.get(..SHORT_SHA_LENGTH).unwrap_or(&self.sha)
    }
}

impl Updates {
    pub(super) fn refresh_shell(&self) {
        let Some(commit) = installed_commit(VERSION) else {
            return;
        };
        if self.is_refreshing_shell.replace(true) {
            self.is_shell_refresh_queued.set(true);
            return;
        }
        self.listeners.notify();
        let url = format!("https://api.github.com/repos/{REPOSITORY}/compare/{commit}...{BRANCH}");
        let updates = self.clone();
        glib::spawn_future_local(async move {
            let body = process::capture_text(process::command(&[
                "curl",
                "-s",
                "--connect-timeout",
                "15",
                "--speed-limit",
                "1",
                "--speed-time",
                "30",
                "--max-time",
                "120",
                "-H",
                "Accept: application/vnd.github+json",
                &url,
            ]))
            .await;
            if let Some((commits_behind, commits)) = body.as_deref().and_then(read_comparison) {
                updates.commits_behind.set(commits_behind);
                updates.commits.replace(commits);
            }
            updates.is_refreshing_shell.set(false);
            updates.listeners.notify();
            if updates.is_shell_refresh_queued.replace(false) {
                updates.refresh_shell();
            }
        });
    }
}

fn installed_commit(version: &str) -> Option<&str> {
    let (count, hash) = version.strip_prefix('r')?.split_once('.')?;
    let is_numeric = !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit());
    (is_numeric && !hash.is_empty()).then_some(hash)
}

fn read_comparison(body: &str) -> Option<(i32, Vec<Commit>)> {
    let comparison = serde_json::from_str::<Value>(body).ok()?;
    if comparison.get("message").and_then(Value::as_str) == Some("Not Found") {
        return Some((0, Vec::new()));
    }
    Some((read_commits_behind(&comparison)?, read_commits(&comparison)))
}

fn read_commits_behind(comparison: &Value) -> Option<i32> {
    let count = comparison.get("ahead_by")?.as_u64()?;
    Some(i32::try_from(count).unwrap_or(i32::MAX))
}

fn read_commits(comparison: &Value) -> Vec<Commit> {
    comparison
        .get("commits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|commit| {
            let sha = commit.get("sha")?.as_str()?;
            let message = commit.pointer("/commit/message")?.as_str()?;
            Some(Commit {
                sha: sha.to_owned(),
                subject: message.lines().next().unwrap_or_default().to_owned(),
            })
        })
        .collect()
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
        let read = |json: &str| read_commits_behind(&serde_json::from_str(json).unwrap());
        assert_eq!(
            read(r#"{"status":"ahead","ahead_by":3,"behind_by":0}"#),
            Some(3)
        );
        assert_eq!(
            read(r#"{"status":"identical","ahead_by":0,"behind_by":0}"#),
            Some(0)
        );
        assert_eq!(
            read(r#"{"status":"diverged","ahead_by":2,"behind_by":5}"#),
            Some(2)
        );
        assert_eq!(read(r#"{"message":"Not Found"}"#), None);
        assert_eq!(read(r#"{"ahead_by":4294967299}"#), Some(i32::MAX));
        assert_eq!(read(r#"{"ahead_by":-5}"#), None);
    }

    #[test]
    fn a_failed_comparison_gives_no_count_and_an_unknown_commit_counts_as_up_to_date() {
        assert_eq!(
            read_comparison(
                r#"{"ahead_by":1,"commits":[{"sha":"e28f698aa","commit":{"message":"Bar: clock"}}]}"#
            ),
            Some((
                1,
                vec![Commit {
                    sha: "e28f698aa".to_owned(),
                    subject: "Bar: clock".to_owned()
                }]
            ))
        );
        assert_eq!(read_comparison(""), None);
        assert_eq!(
            read_comparison(r#"{"message":"API rate limit exceeded for 127.0.0.1."}"#),
            None
        );
        assert_eq!(
            read_comparison(r#"{"message":"Not Found"}"#),
            Some((0, Vec::new()))
        );
    }

    #[test]
    fn the_commits_are_the_hashes_and_first_message_lines_of_the_compared_commits() {
        let comparison = serde_json::from_str(
            r#"{"ahead_by":3,"commits":[
                {"sha":"0123456789abcdef","commit":{"message":"Theme: redraw every widget\n\nBody"}},
                {"sha":"fedcba9876543210","commit":{"message":"Docs: links"}},
                {"commit":{"message":"No hash"}}
            ]}"#,
        )
        .unwrap();
        let commits = read_commits(&comparison);
        assert_eq!(
            commits
                .iter()
                .map(|commit| (commit.sha.as_str(), commit.subject.as_str()))
                .collect::<Vec<_>>(),
            [
                ("0123456789abcdef", "Theme: redraw every widget"),
                ("fedcba9876543210", "Docs: links")
            ]
        );
        assert_eq!(commits[0].short_sha(), "0123456");
        assert!(
            read_commits(&serde_json::from_str(r#"{"message":"Not Found"}"#).unwrap()).is_empty()
        );
    }

    #[test]
    fn a_hash_shorter_than_its_short_form_is_shown_whole() {
        let commit = Commit {
            sha: "abc".to_owned(),
            subject: String::new(),
        };
        assert_eq!(commit.short_sha(), "abc");
    }
}
